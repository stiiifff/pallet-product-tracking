#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
// use fixed::types::I16F16;
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch, ensure, sp_runtime::RuntimeDebug,
};
use frame_system::{self as system, ensure_signed};
use product_registry::ProductId;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// Custom types
pub type EventId = Vec<u8>;
pub type EventType = Vec<u8>;
pub type ShipmentId = Vec<u8>;
pub type DeviceId = Vec<u8>;
pub type ReadingType = Vec<u8>;
pub type ReadingValue = Vec<u8>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Shipment<AccountId, Moment> {
    id: ShipmentId,
    owner: AccountId,
    products: Vec<ProductId>,
    registered: Moment,
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
    trait Store for Module<T: Trait> as TemplateModule {
        pub EventCount: u64;
        pub AllEvents: map hasher(blake2_128_concat) u64 => Option<EventRecord<T::Moment>>;
        pub EventIndices get(fn event_by_id): map hasher(blake2_128_concat) EventId => Option<u64>;
        pub Shipments get(fn shipment_by_id): map hasher(blake2_128_concat) ShipmentId => Option<Shipment<T::AccountId, T::Moment>>;
        pub EventsOfShipment get(fn events_by_shipment): map hasher(blake2_128_concat) ShipmentId => Vec<u64>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
    {
        EventRecorded(AccountId),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        EventRecordExists,
        EventRecordMaxExceeded,
        ShipmentIdUnknown
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

            // Validate product IDs
            // Self::validate_product_id(&id)?;

            // Create a product instance
            // let product = Self::new_product()
            //     .identified_by(id.clone())
            //     .owned_by(owner.clone())
            //     .registered_on(<timestamp::Module<T>>::now())
            //     .with_props(props)
            //     .build();

            // // Add product & ownerOf (2 DB writes)
            // <Products<T>>::insert(&id, product);
            // <OwnerOf<T>>::insert(&id, &owner);

            // Self::deposit_event(RawEvent::ProductRegistered(who, id, owner));

            Ok(())
        }

        #[weight = 10_000]
        pub fn record_event(origin, event: EventRecord<T::Moment>) -> dispatch::DispatchResult {
            let who = ensure_signed(origin)?;
            // Validate extrinsic data (no storage access)
            // ...

            // Storage checks
            // --------------
            // Get event count (1 DB read)
            let event_count = EventCount::get();
            let event_idx = event_count.checked_add(1).ok_or(Error::<T>::EventRecordMaxExceeded)?;
            // Check event doesn't exist yet (1 DB read)
            let event_key = EventIndices::hashed_key_for(&event.event_id);
            ensure!(!EventIndices::contains_key(&event_key), Error::<T>::EventRecordExists);
            // Check shipment has been registered (1 DB read)
            let shipment_key = EventsOfShipment::hashed_key_for(&event.shipment_id);
            ensure!(<Shipments<T>>::contains_key(&shipment_key), Error::<T>::ShipmentIdUnknown);

            // Storage writes
            // --------------
            EventCount::put(event_idx);
            <AllEvents<T>>::insert(event_idx, event);
            EventIndices::insert(event_key, event_idx);
            EventsOfShipment::append(shipment_key, event_idx);

            Self::deposit_event(RawEvent::EventRecorded(who));

            Ok(())
        }
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
        }
    }
}
