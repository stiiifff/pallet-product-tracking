#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
// use fixed::types::I16F16;
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch, sp_runtime::RuntimeDebug,
};
use frame_system::{self as system, ensure_signed};
use product_registry::ProductId;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// Custom types
pub type EventType = Vec<u8>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct EventRecord<Moment> {
    event: EventType,
    products: Vec<ProductId>,
    timestamp: Moment,
    location: Vec<u8>,
    readings: Vec<u8>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ReadPoint {
    pub latitude: Vec<u8>,
}

pub trait Trait: system::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
    trait Store for Module<T: Trait> as TemplateModule {
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
    {
        EventTracked(AccountId),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;
        fn deposit_event() = default;

        #[weight = 10_000]
        pub fn record_event(origin, event: EventRecord) -> dispatch::DispatchResult {
            let who = ensure_signed(origin)?;

            Ok(())
        }
    }
}
