use codec::{ Decode, Encode };
use fixed::types::I16F16;
use frame_support::{ sp_runtime::RuntimeDebug, sp_std::prelude::* };
use product_registry::ProductId;

// Custom types
pub type Identifier = Vec<u8>;
pub type Decimal = I16F16;
pub type ShipmentId = Identifier;
pub type ShippingEventIndex = u128;
pub type DeviceId = Identifier;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum ShipmentStatus {
    Pending,
    InTransit,
    Delivered,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Shipment<AccountId, Moment> {
    pub id: ShipmentId,
    pub owner: AccountId,
    pub status: ShipmentStatus,
    pub products: Vec<ProductId>,
    pub registered: Moment,
    pub delivered: Option<Moment>,
}

impl<AccountId, Moment> Shipment<AccountId, Moment> {
    pub fn pickup(mut self) -> Self {
        self.status = ShipmentStatus::InTransit;
        self
    }

    pub fn deliver(mut self, delivered_on: Moment) -> Self {
        self.status = ShipmentStatus::Delivered;
        self.delivered = Some(delivered_on);
        self
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum ShippingOperation {
    Pickup,
    Scan,
    Deliver,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum ShippingEventType {
    ShipmentRegistration,
    ShipmentPickup,
    ShipmentScan,
    ShipmentDeliver,
}

impl From<ShippingOperation> for ShippingEventType {
    fn from(op: ShippingOperation) -> Self {
        match op {
            ShippingOperation::Pickup => ShippingEventType::ShipmentPickup,
            ShippingOperation::Scan => ShippingEventType::ShipmentScan,
            ShippingOperation::Deliver => ShippingEventType::ShipmentDeliver,
        }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ShippingEvent<Moment> {
    pub event_type: ShippingEventType,
    pub shipment_id: ShipmentId,
    pub location: Option<ReadPoint>,
    pub readings: Vec<Reading<Moment>>,
    pub timestamp: Moment,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct ReadPoint {
    pub latitude: Decimal,
    pub longitude: Decimal,
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
    pub device_id: DeviceId,
    pub reading_type: ReadingType,
    pub timestamp: Moment,
    pub value: Decimal,
}

// #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
// pub enum OcwTaskType {
//     ShipmentRegistration,
//     ShipmentPickup,
//     ShipmentDelivery,
// }

// impl OcwTaskType {
//     pub fn from_shipping_event_type(
//         shipping_event_type: &ShippingEventType,
//     ) -> Result<OcwTaskType, &'static str> {
//         match shipping_event_type {
//             ShippingEventType::ShipmentRegistered => Ok(OcwTaskType::ShipmentRegistration),
//             ShippingEventType::ShipmentPickup => Ok(OcwTaskType::ShipmentPickup),
//             ShippingEventType::ShipmentDelivery => Ok(OcwTaskType::ShipmentDelivery),
//             ShippingEventType::SensorReading => Err("Unsupported shipping event type conversion"),
//         }
//     }
// }

// #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
// pub enum OcwTaskPayload<AccountId, Moment> {
//     Shipment(Shipment<AccountId, Moment>),
//     ShippingEvent(ShippingEvent<Moment>),
// }

// #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
// pub struct OcwTask<AccountId, Moment> {
//     r#type: OcwTaskType,
//     payload: OcwTaskPayload<AccountId, Moment>,
// }

// impl<A, M> fmt::Display for OcwTask<A, M>
// where
//     A: fmt::Debug,
//     M: fmt::Debug,
// {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{:?}", self)
//     }
// }
