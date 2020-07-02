#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch, ensure, sp_runtime::RuntimeDebug,
};
use product_registry::ProductId;
use sp_std::{prelude::*, vec::Vec};
use system::{self as system, ensure_signed};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// General constraints to limit data size
// Note: these could also be passed as trait config parameters
pub const SHIPMENT_ID_MAX_LENGTH: usize = 20;
pub const SHIPMENT_MAX_PRODUCTS: usize = 10;

// Custom types
pub type EventId = Vec<u8>;
pub type EventIndex = u64;
pub type EventType = Vec<u8>;
pub type ShipmentId = Vec<u8>;
pub type ShipmentIndex = u64;
pub type DeviceId = Vec<u8>;
pub type ReadingType = Vec<u8>;
pub type ReadingValue = Vec<u8>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Shipment<AccountId, Moment> {
    id: ShipmentId,
    owner: AccountId,
    registered: Moment,
    //TODO: we probably need a shipment status
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct EventRecord<Moment> {
    event_id: EventId,
    event_type: EventType,
    shipment_id: ShipmentId,
    location: Option<ReadPoint>,
    readings: Vec<Reading<Moment>>,
    timestamp: Moment,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ReadPoint {
    //TODO: these should use fixed-point numbers
    latitude: Vec<u8>,
    longitude: Vec<u8>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Reading<Moment> {
    device_id: DeviceId,
    reading_type: ReadingType,
    timestamp: Moment,
    value: ReadingValue,
}

pub trait Trait: system::Trait + timestamp::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
    trait Store for Module<T: Trait> as ProductTracking {
        pub ShipmentCount: u64;
        pub Shipments: map hasher(twox_64_concat) ShipmentIndex => Option<Shipment<T::AccountId, T::Moment>>;
        pub ShipmentIndices get(fn shipment_by_id): map hasher(blake2_128_concat) ShipmentId => Option<ShipmentIndex>;
        pub ShipmentsOfOrganization: map hasher(blake2_128_concat) T::AccountId => Vec<ShipmentIndex>;

        pub EventCount: u64;
        pub AllEvents: map hasher(blake2_128_concat) EventIndex => Option<EventRecord<T::Moment>>;
        pub EventIndices get(fn event_by_id): map hasher(blake2_128_concat) EventId => Option<EventIndex>;
        pub EventsOfShipment get(fn events_by_shipment): map hasher(blake2_128_concat) ShipmentId => Vec<EventIndex>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
    {
        EventRecorded(AccountId, EventId, ShipmentId),
        ShipmentRegistered(AccountId, ShipmentId, AccountId),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        //TODO: Too granular ?
        EventRecordExists,
        EventRecordMaxExceeded,
        ShipmentIdExists,
        ShipmentIdMissing,
        ShipmentIdTooLong,
        ShipmentIdUnknown,
        ShipmentMaxExceeded,
        ShipmentTooManyProducts
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

            // Validate shipment ID
            Self::validate_shipment_id(&id)?;

            // Validate shipment products
            Self::validate_shipment_products(&products)?;

            // Check shipment doesn't exist yet (1 DB read)
            Self::validate_new_shipment(&id)?;

            // Storage checks
            // --------------
            // Get shipment count (1 DB read)
            let shipment_count = ShipmentCount::get();
            let shipment_idx = shipment_count.checked_add(1).ok_or(Error::<T>::ShipmentMaxExceeded)?;

            // Create a shipment instance
            let shipment = Self::new_shipment()
                .identified_by(id.clone())
                .owned_by(owner.clone())
                .registered_on(<timestamp::Module<T>>::now())
                .with_products(products)
                .build();

            // Storage writes
            // --------------
            // Add shipment (1 DB write)
            ShipmentCount::put(shipment_idx);
            <Shipments<T>>::insert(shipment_idx, shipment);
            ShipmentIndices::insert(&id, shipment_idx);
            <ShipmentsOfOrganization<T>>::append(&owner, shipment_idx);

            Self::deposit_event(RawEvent::ShipmentRegistered(who, id, owner));

            Ok(())
        }

        #[weight = 10_000]
        pub fn record_event(origin, event: EventRecord<T::Moment>) -> dispatch::DispatchResult {
            let who = ensure_signed(origin)?;
            // Validate extrinsic data (no storage access)
            // -----------------------
            // Validate shipment ID
            Self::validate_shipment_id(&event.shipment_id)?;
            let shipment_id = event.shipment_id.clone();

            // Storage checks
            // --------------
            // Get event count (1 DB read)
            let event_count = EventCount::get();
            let event_idx = event_count.checked_add(1).ok_or(Error::<T>::EventRecordMaxExceeded)?;
            // Check event doesn't exist yet (1 DB read)
            let event_key = EventIndices::hashed_key_for(&event.event_id);
            ensure!(!EventIndices::contains_key(&event_key), Error::<T>::EventRecordExists);
            let event_id = event.event_id.clone();
            // Check shipment has been registered (1 DB read)
            let shipment_key = EventsOfShipment::hashed_key_for(&event.shipment_id);
            ensure!(ShipmentIndices::contains_key(&shipment_key), Error::<T>::ShipmentIdUnknown);

            // Storage writes
            // --------------
            EventCount::put(event_idx);
            <AllEvents<T>>::insert(event_idx, event);
            EventIndices::insert(event_key, event_idx);
            EventsOfShipment::append(shipment_key, event_idx);

            Self::deposit_event(RawEvent::EventRecorded(who, event_id, shipment_id));

            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
    // Helper methods
    fn new_shipment() -> ShipmentBuilder<T::AccountId, T::Moment> {
        ShipmentBuilder::<T::AccountId, T::Moment>::default()
    }

    pub fn validate_shipment_id(id: &[u8]) -> Result<(), Error<T>> {
        // Basic shipment ID validation
        ensure!(!id.is_empty(), Error::<T>::ShipmentIdMissing);
        ensure!(
            id.len() <= SHIPMENT_ID_MAX_LENGTH,
            Error::<T>::ShipmentIdTooLong
        );
        Ok(())
    }

    pub fn validate_new_shipment(id: &[u8]) -> Result<(), Error<T>> {
        // Product existence check
        ensure!(
            !ShipmentIndices::contains_key(id),
            Error::<T>::ShipmentIdExists
        );
        Ok(())
    }

    pub fn validate_shipment_products(props: &Vec<ProductId>) -> Result<(), Error<T>> {
        ensure!(
            props.len() <= SHIPMENT_MAX_PRODUCTS,
            Error::<T>::ShipmentTooManyProducts,
        );
        // TODO: Validate products IDs
        // for prop in props {
        // 	ensure!(
        // 		prop.name().len() <= PRODUCT_PROP_NAME_MAX_LENGTH,
        // 		Error::<T>::ProductInvalidPropName
        // 	);
        // 	ensure!(
        // 		prop.value().len() <= PRODUCT_PROP_VALUE_MAX_LENGTH,
        // 		Error::<T>::ProductInvalidPropValue
        // 	);
        // }
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
            registered: self.registered,
        }
    }
}
