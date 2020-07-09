#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use fixed::types::U16F16;
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch, ensure, sp_runtime::RuntimeDebug,
    sp_std::prelude::*,
};
use frame_system::{self as system, ensure_signed};
use product_registry::ProductId;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// General constraints to limit data size
// Note: these could also be passed as trait config parameters
pub const IDENTIFIER_MAX_LENGTH: usize = 10;
pub const SHIPMENT_MAX_PRODUCTS: usize = 10;

// Custom types
pub type Identifier = Vec<u8>;
pub type Decimal = U16F16;
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
    SensorReading,
    ShipmentDelivery,
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

pub trait Trait: system::Trait + timestamp::Trait {
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
            <Shipments<T>>::insert(&id, shipment);
            <ShipmentsOfOrganization<T>>::append(&owner, &id);

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
            <AllEvents<T>>::insert(event_idx, event);
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
                    Self::deposit_event(RawEvent::ShipmentStatusUpdated(shipment_id, new_status));
                },
            }

            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
    // Helper methods
    fn new_shipment() -> ShipmentBuilder<T::AccountId, T::Moment> {
        ShipmentBuilder::<T::AccountId, T::Moment>::default()
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
