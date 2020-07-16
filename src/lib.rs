#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch, ensure, sp_std::prelude::*,
};
use frame_system::{ self as system, ensure_signed, offchain::SendTransactionTypes };

use product_registry::ProductId;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod types;
use crate::types::*;

mod builders;
use crate::builders::*;

// General constraints to limit data size
// Note: these could also be passed as trait config parameters
pub const IDENTIFIER_MAX_LENGTH: usize = 10;
pub const SHIPMENT_MAX_PRODUCTS: usize = 10;
pub const LISTENER_ENDPOINT: &'static str = "http://localhost:3005";

pub trait Trait: system::Trait + timestamp::Trait + SendTransactionTypes<Call<Self>> {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
    trait Store for Module<T: Trait> as ProductTracking {
        // Shipments
        pub Shipments get(fn shipment_by_id): map hasher(blake2_128_concat) ShipmentId => Option<Shipment<T::AccountId, T::Moment>>;
        pub ShipmentsOfOrganization get(fn shipments_of_org): map hasher(blake2_128_concat) T::AccountId => Vec<ShipmentId>;

        // Shipping events
        pub EventCount get(fn event_count): u128 = 0;
        pub AllEvents get(fn event_by_idx): map hasher(blake2_128_concat) ShippingEventIndex => Option<ShippingEvent<T::Moment>>;
        pub EventsOfShipment get(fn events_of_shipment): map hasher(blake2_128_concat) ShipmentId => Vec<ShippingEventIndex>;

        // Off-chain Worker notifications
        pub OcwNotifications get (fn ocw_notifications): map hasher(identity) T::BlockNumber => Vec<ShippingEventIndex>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
    {
        ShipmentRegistered(AccountId, ShipmentId, AccountId),
        ShipmentStatusUpdated(AccountId, ShipmentId, ShippingEventIndex, ShipmentStatus),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        InvalidOrMissingIdentifier,
        ShipmentAlreadyExists,
        ShipmentHasBeenDelivered,
        ShipmentIsInTransit,
        ShipmentIsUnknown,
        ShipmentHasTooManyProducts,
        ShippingEventAlreadyExists,
        ShippingEventMaxExceeded,
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;
        fn deposit_event() = default;

        #[weight = 10_000]
        pub fn register_shipment(origin, id: ShipmentId, owner: T::AccountId, products: Vec<ProductId>) -> dispatch::DispatchResult {
            let who = ensure_signed(origin)?;

            // TODO: assuming owner is a DID representing an organization,
            //       validate tx sender is owner or delegate of organization.

            // Validate format of shipment ID
            Self::validate_identifier(&id)?;

            // Validate shipment products
            Self::validate_shipment_products(&products)?;

            // Check shipment doesn't exist yet (1 DB read)
            Self::validate_new_shipment(&id)?;

            // Create a shipment instance
            let shipment = Self::new_shipment()
                .identified_by(id.clone())
                .owned_by(owner.clone())
                .registered_at(<timestamp::Module<T>>::now())
                .with_products(products)
                .build();
            let status = shipment.status.clone();

            // Create shipping event
            let event = Self::new_shipping_event()
                .of_type(ShippingEventType::ShipmentRegistration)
                .for_shipment(id.clone())
                .at_location(None)
                .with_readings(vec![])
                .at_time(shipment.registered)
                .build();

            // Storage writes
            // --------------
            // Add shipment (2 DB write)
            <Shipments<T>>::insert(&id, shipment);
            <ShipmentsOfOrganization<T>>::append(&owner, &id);
            // Store shipping event (1 DB read, 3 DB writes)
            let event_idx = Self::store_event(event)?;
            // Update offchain notifications (1 DB write)
            <OcwNotifications<T>>::append(<system::Module<T>>::block_number(), event_idx);

            // Raise events
            Self::deposit_event(RawEvent::ShipmentRegistered(who.clone(), id.clone(), owner));
            Self::deposit_event(RawEvent::ShipmentStatusUpdated(who, id, event_idx, status));

            Ok(())
        }

        #[weight = 10_000]
        pub fn track_shipment(origin, id: ShipmentId, operation: ShippingOperation, timestamp: T::Moment, location: Option<ReadPoint>, readings: Option<Vec<Reading<T::Moment>>>) -> dispatch::DispatchResult {
            let who = ensure_signed(origin)?;

            // Validate format of shipment ID
            Self::validate_identifier(&id)?;

            // Check shipment is known (1 DB read) & do transition checks
            let mut shipment = match <Shipments<T>>::get(&id) {
                Some(shipment) => match shipment.status {
                    ShipmentStatus::Delivered => Err(<Error<T>>::ShipmentHasBeenDelivered),
                    ShipmentStatus::InTransit if operation == ShippingOperation::Pickup =>
                        Err(<Error<T>>::ShipmentIsInTransit),
                    _ => Ok(shipment)
                }
                None => Err(<Error<T>>::ShipmentIsUnknown)
            }?;

            // Update shipment status
            shipment = match operation {
                ShippingOperation::Pickup => shipment.pickup(),
                ShippingOperation::Deliver => shipment.deliver(timestamp),
                _ => shipment,
            };
            let status = shipment.status.clone();

            // Create shipping event
            let event = Self::new_shipping_event()
                .of_type(operation.clone().into())
                .for_shipment(id.clone())
                .at_location(location)
                .with_readings(readings.unwrap_or(vec![]))
                .at_time(timestamp)
                .build();

            // Storage writes
            // --------------
            // Store shipping event (1 DB read, 3 DB writes)
            let event_idx = Self::store_event(event)?;
            // Update offchain notifications (1 DB write)
            <OcwNotifications<T>>::append(<system::Module<T>>::block_number(), event_idx);

            if operation != ShippingOperation::Scan {
                // Update shipment (1 DB write)
                <Shipments<T>>::insert(&id, shipment);
                // Raise events
                Self::deposit_event(RawEvent::ShipmentStatusUpdated(who, id, event_idx, status));
            }

            Ok(())
        }

        // fn offchain_worker(_block_number: T::BlockNumber) {
        //     if Self::ocw_tasks().len() == 0 { return; }
        //     let mut tasks: Vec<OcwTask<T::AccountId, T::Moment>> = <OcwTasks<T>>::get();

        //     while tasks.len() > 0 {
        //         let task = tasks.remove(0);
        //         debug::info!("ocw task: {:?}", task);
        //         let _ = Self::notify_listener(&task).map_err(|e| {
        //             debug::error!("Error notifying listener. Err: {:?}", e);
        //         });
        //     }

        //     // Submit a transaction back on-chain to clear the task queue
        //     let call = Call::clear_ocwtasks();

        //     let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into()).map_err(|e| {
        //         debug::error!("Failed in submitting tx for clearing ocw taskqueue. Err: {:?}", e);
        //     });
        // }
    }
}

impl<T: Trait> Module<T> {
    // Helper methods
    fn new_shipment() -> ShipmentBuilder<T::AccountId, T::Moment> {
        ShipmentBuilder::<T::AccountId, T::Moment>::default()
    }

    fn new_shipping_event() -> ShippingEventBuilder<T::Moment> {
        ShippingEventBuilder::<T::Moment>::default()
    }

    // fn notify_listener(task: &OcwTask<T::AccountId, T::Moment>) -> Result<(), &'static str> {
    //     let request =
    //         sp_runtime::offchain::http::Request::post(&LISTENER_ENDPOINT, vec![task.to_string()]);

    //     let timeout =
    //         sp_io::offchain::timestamp().add(sp_runtime::offchain::Duration::from_millis(3000));

    //     let pending = request
    //         .add_header(&"Content-Type", &"text/plain")
    //         .deadline(timeout) // Setting the timeout time
    //         .send() // Sending the request out by the host
    //         .map_err(|_| "http post request building error")?;

    //     let response = pending
    //         .try_wait(timeout)
    //         .map_err(|_| "http post request sent error")?
    //         .map_err(|_| "http post request sent error")?;

    //     if response.code != 200 {
    //         return Err("http response error");
    //     }

    //     Ok(())
    // }

    pub fn store_event(event: ShippingEvent<T::Moment>) -> Result<ShippingEventIndex, Error<T>> {
        let event_idx = EventCount::get()
            .checked_add(1)
            .ok_or(Error::<T>::ShippingEventMaxExceeded)?;

        EventCount::put(event_idx);
        EventsOfShipment::append(&event.shipment_id, event_idx);
        <AllEvents<T>>::insert(event_idx, event);

        Ok(event_idx)
    }

    pub fn validate_identifier(id: &[u8]) -> Result<(), Error<T>> {
        // Basic identifier validation
        ensure!(!id.is_empty(), Error::<T>::InvalidOrMissingIdentifier);
        ensure!(
            id.len() <= IDENTIFIER_MAX_LENGTH,
            Error::<T>::InvalidOrMissingIdentifier
        );
        Ok(())
    }

    pub fn validate_new_shipment(id: &[u8]) -> Result<(), Error<T>> {
        // Shipment existence check
        ensure!(
            !<Shipments<T>>::contains_key(id),
            Error::<T>::ShipmentAlreadyExists
        );
        Ok(())
    }

    pub fn validate_shipment_products(props: &[ProductId]) -> Result<(), Error<T>> {
        ensure!(
            props.len() <= SHIPMENT_MAX_PRODUCTS,
            Error::<T>::ShipmentHasTooManyProducts,
        );
        Ok(())
    }
}
