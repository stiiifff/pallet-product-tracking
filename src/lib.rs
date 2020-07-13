#![cfg_attr(not(feature = "std"), no_std)]

use codec::{alloc::string::ToString, Decode, Encode};
use core::fmt;
use fixed::types::I16F16;
use frame_support::{
    debug, decl_error, decl_event, decl_module, decl_storage, dispatch, ensure,
    sp_runtime::RuntimeDebug, sp_std::prelude::*,
};
use frame_system::{
    self as system, ensure_none, ensure_signed,
    offchain::{SendTransactionTypes, SubmitTransaction},
};
use sp_runtime::transaction_validity::{
    InvalidTransaction, TransactionSource, TransactionValidity, ValidTransaction,
};

use product_registry::ProductId;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// General constraints to limit data size
// Note: these could also be passed as trait config parameters
pub const IDENTIFIER_MAX_LENGTH: usize = 10;
pub const SHIPMENT_MAX_PRODUCTS: usize = 10;

pub const LISTENER_ENDPOINT: &'static str = "http://localhost:3005";

// Custom types
pub type Identifier = Vec<u8>;
pub type Decimal = I16F16;
pub type ShipmentId = Identifier;
pub type ShippingEventId = Identifier;
pub type ShippingEventIndex = u64;
pub type DeviceId = Identifier;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum ShipmentStatus {
    Pending,
    InTransit,
    Delivered,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Shipment<AccountId, Moment> {
    id: ShipmentId,
    owner: AccountId,
    status: ShipmentStatus,
    products: Vec<ProductId>,
    registered: Moment,
    delivered: Option<Moment>,
}

impl<AccountId, Moment> Shipment<AccountId, Moment> {
    fn pickup(mut self) -> Self {
        self.status = ShipmentStatus::InTransit;
        self
    }

    fn deliver(mut self, delivered_on: Moment) -> Self {
        self.status = ShipmentStatus::Delivered;
        self.delivered = Some(delivered_on);
        self
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum ShippingEventType {
    ShipmentPickup,
    ShipmentDelivery,
    SensorReading,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ShippingEvent<Moment> {
    id: ShippingEventId,
    event_type: ShippingEventType,
    shipment_id: ShipmentId,
    location: Option<ReadPoint>,
    readings: Vec<Reading<Moment>>,
    timestamp: Moment,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ReadPoint {
    latitude: Decimal,
    longitude: Decimal,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum ReadingType {
    Humidity,
    Pressure,
    Shock,
    Tilt,
    Temperature,
    Vibration,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Reading<Moment> {
    device_id: DeviceId,
    reading_type: ReadingType,
    timestamp: Moment,
    value: Decimal,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum OcwTaskType {
    ShipmentRegistration,
    ShipmentPickup,
    ShipmentDelivery,
}

impl OcwTaskType {
    pub fn from_shipping_event_type(
        shipping_event_type: &ShippingEventType,
    ) -> Result<OcwTaskType, &'static str> {
        match shipping_event_type {
            ShippingEventType::ShipmentPickup => Ok(OcwTaskType::ShipmentPickup),
            ShippingEventType::ShipmentDelivery => Ok(OcwTaskType::ShipmentDelivery),
            ShippingEventType::SensorReading => Err("Unsupported shipping event type conversion"),
        }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum OcwTaskPayload<AccountId, Moment> {
    Shipment(Shipment<AccountId, Moment>),
    ShippingEvent(ShippingEvent<Moment>),
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct OcwTask<AccountId, Moment> {
    r#type: OcwTaskType,
    payload: OcwTaskPayload<AccountId, Moment>,
}

impl<A, M> fmt::Display for OcwTask<A, M>
where
    A: fmt::Debug,
    M: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait Trait: system::Trait + timestamp::Trait + SendTransactionTypes<Call<Self>> {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
    trait Store for Module<T: Trait> as ProductTracking {
        pub Shipments get(fn shipment_by_id): map hasher(blake2_128_concat) ShipmentId => Option<Shipment<T::AccountId, T::Moment>>;
        pub ShipmentsOfOrganization get(fn shipments_of_org): map hasher(blake2_128_concat) T::AccountId => Vec<ShipmentId>;

        pub EventCount get(fn event_count): u64;
        pub AllEvents get(fn event_by_idx): map hasher(blake2_128_concat) ShippingEventIndex => Option<ShippingEvent<T::Moment>>;
        pub EventIndices get(fn event_idx_from_id): map hasher(blake2_128_concat) ShippingEventId => Option<ShippingEventIndex>;
        pub EventsOfShipment get(fn events_of_shipment): map hasher(blake2_128_concat) ShipmentId => Vec<ShippingEventIndex>;
        // OCW tasks queue
        pub OcwTasks get(fn ocw_tasks): Vec<OcwTask<T::AccountId, T::Moment>>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
    {
        ShipmentRegistered(AccountId, ShipmentId, AccountId),
        ShipmentStatusUpdated(ShipmentId, ShipmentStatus),
        ShippingEventRecorded(AccountId, ShippingEventId, ShipmentId, ShippingEventType),
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
                .registered_on(<timestamp::Module<T>>::now())
                .with_products(products)
                .build();
            let status = shipment.status.clone();

            // Storage writes
            // --------------
            // Add shipment (1 DB write)
            <Shipments<T>>::insert(&id, shipment.clone());
            <ShipmentsOfOrganization<T>>::append(&owner, &id);

                  // Inserting task to the ocw tasks queue
                  <OcwTasks<T>>::append(OcwTask {
                        r#type: OcwTaskType::ShipmentRegistration,
                        payload: OcwTaskPayload::Shipment(shipment),
                  });

            Self::deposit_event(RawEvent::ShipmentRegistered(who, id.clone(), owner));
            Self::deposit_event(RawEvent::ShipmentStatusUpdated(id, status));

            Ok(())
        }

        #[weight = 10_000]
        pub fn record_event(origin, event: ShippingEvent<T::Moment>) -> dispatch::DispatchResult {
            let who = ensure_signed(origin)?;

            // Validate extrinsic data (no storage access)
            // -----------------------
            // Validate format of event & shipment ID
            Self::validate_identifier(&event.id)?;
            Self::validate_identifier(&event.shipment_id)?;

            let event_id = event.id.clone();
            let event_type = event.event_type.clone();
            let shipment_id = event.shipment_id.clone();

            // Storage checks
            // --------------
            // Get event count (1 DB read)
            let event_count = EventCount::get();
            let event_idx = event_count.checked_add(1).ok_or(Error::<T>::ShippingEventMaxExceeded)?;
            // Check event doesn't exist yet (1 DB read)
            Self::validate_new_shipping_event(&event_id)?;

            // Check shipment is known (1 DB read)
            // Additionnally, we refuse some shipping events based on the shipment's status
            let mut shipment = match <Shipments<T>>::get(&shipment_id)
            {
                Some(shipment) => {
                    match shipment.status {
                        ShipmentStatus::Delivered => Err(<Error<T>>::ShipmentHasBeenDelivered),
                        ShipmentStatus::InTransit if event_type == ShippingEventType::ShipmentPickup => Err(<Error<T>>::ShipmentIsInTransit),
                        _ => Ok(shipment)
                    }
                }
                None => Err(<Error<T>>::ShipmentIsUnknown)
            }?;

            // Storage writes
            // --------------
            EventCount::put(event_idx);
            <AllEvents<T>>::insert(event_idx, event.clone());
            EventIndices::insert(&event_id, event_idx);
            EventsOfShipment::append(&shipment_id, event_idx);

            Self::deposit_event(RawEvent::ShippingEventRecorded(who, event_id, shipment_id.clone(), event_type.clone()));

            match event_type {
                ShippingEventType::SensorReading => { /* Do nothing */ },
                _ => {
                    shipment = match event_type {
                        ShippingEventType::ShipmentPickup => shipment.pickup(),
                        ShippingEventType::ShipmentDelivery => shipment.deliver(<timestamp::Module<T>>::now()),
                        _ => unreachable!()
                    };
                    let new_status = shipment.status.clone();
                    <Shipments<T>>::insert(&shipment_id, shipment);

                              // Inserting task to the ocw tasks queue
                              <OcwTasks<T>>::append(OcwTask {
                                    r#type: OcwTaskType::from_shipping_event_type(&event_type)?,
                                    payload: OcwTaskPayload::ShippingEvent(event),
                              });

                    Self::deposit_event(RawEvent::ShipmentStatusUpdated(shipment_id, new_status));
                },
            }
            Ok(())
        }

        #[weight = 0]
        pub fn clear_ocwtasks(origin) -> dispatch::DispatchResult {
            // Using unsigned_tx with signed payload to call this function, to ensure
            //   only authroized chain node can call this
            ensure_none(origin)?;
            <OcwTasks<T>>::kill();
            Ok(())
        }

        fn offchain_worker(_block_number: T::BlockNumber) {
            if Self::ocw_tasks().len() == 0 { return; }
            let mut tasks: Vec<OcwTask<T::AccountId, T::Moment>> = <OcwTasks<T>>::get();

            while tasks.len() > 0 {
                let task = tasks.remove(0);
                debug::info!("ocw task: {:?}", task);
                let _ = Self::notify_listener(&task).map_err(|e| {
                    debug::error!("Error notifying listener. Err: {:?}", e);
                });
            }

            // Submit a transaction back on-chain to clear the task queue
            let call = Call::clear_ocwtasks();

            let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into()).map_err(|e| {
                debug::error!("Failed in submitting tx for clearing ocw taskqueue. Err: {:?}", e);
            });
        }
    }
}

impl<T: Trait> Module<T> {
    // Helper methods
    fn new_shipment() -> ShipmentBuilder<T::AccountId, T::Moment> {
        ShipmentBuilder::<T::AccountId, T::Moment>::default()
    }

    fn notify_listener(task: &OcwTask<T::AccountId, T::Moment>) -> Result<(), &'static str> {
        let request =
            sp_runtime::offchain::http::Request::post(&LISTENER_ENDPOINT, vec![task.to_string()]);

        let timeout =
            sp_io::offchain::timestamp().add(sp_runtime::offchain::Duration::from_millis(3000));

        let pending = request
            .add_header(&"Content-Type", &"text/plain")
            .deadline(timeout) // Setting the timeout time
            .send() // Sending the request out by the host
            .map_err(|_| "http post request building error")?;

        let response = pending
            .try_wait(timeout)
            .map_err(|_| "http post request sent error")?
            .map_err(|_| "http post request sent error")?;

        if response.code != 200 {
            return Err("http response error");
        }

        Ok(())
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

    pub fn validate_new_shipping_event(id: &[u8]) -> Result<(), Error<T>> {
        // Shipping event existence check
        // let event_key = EventIndices::hashed_key_for(&event_id);
        ensure!(
            !EventIndices::contains_key(id),
            Error::<T>::ShippingEventAlreadyExists
        );
        Ok(())
    }
}

#[derive(Default)]
pub struct ShipmentBuilder<AccountId, Moment>
where
    AccountId: Default,
    Moment: Default,
{
    id: ShipmentId,
    owner: AccountId,
    products: Vec<ProductId>,
    registered: Moment,
}

impl<AccountId, Moment> ShipmentBuilder<AccountId, Moment>
where
    AccountId: Default,
    Moment: Default,
{
    pub fn identified_by(mut self, id: ShipmentId) -> Self {
        self.id = id;
        self
    }

    pub fn owned_by(mut self, owner: AccountId) -> Self {
        self.owner = owner;
        self
    }

    pub fn with_products(mut self, products: Vec<ProductId>) -> Self {
        self.products = products;
        self
    }

    pub fn registered_on(mut self, registered: Moment) -> Self {
        self.registered = registered;
        self
    }

    pub fn build(self) -> Shipment<AccountId, Moment> {
        Shipment::<AccountId, Moment> {
            id: self.id,
            owner: self.owner,
            products: self.products,
            registered: self.registered,
            status: ShipmentStatus::Pending,
            delivered: None,
        }
    }
}

// To allow the module submitting unsigned transaction
impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
    type Call = Call<T>;

    fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
        if let Call::clear_ocwtasks() = call {
            // TODO: validate the signed payload and verify the payload here
            ValidTransaction::with_tag_prefix("product-tracking-ocw")
                .priority(100)
                .and_provides([b"clear_ocwtasks"])
                .longevity(3)
                .propagate(true)
                .build()
        } else {
            InvalidTransaction::Call.into()
        }
    }
}
