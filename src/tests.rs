// Tests to be written here

use super::*;
use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok, dispatch};

pub fn store_test_shipment<T: Trait>(id: ShipmentId, owner: T::AccountId, registered: T::Moment) {
    Shipments::<T>::insert(
        id.clone(),
        Shipment {
            id,
            owner,
            products: vec![],
            registered,
        },
    );
}

const TEST_PRODUCT_ID: &str = "00012345600012";
const TEST_SHIPMENT_ID: &str = "000123456";
const TEST_ORGANIZATION: &str = "Northwind";
const TEST_SENDER: &str = "Alice";
const LONG_VALUE : &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Donec aliquam ut tortor nec congue. Pellente";

// #[test]
// fn create_product_without_props() {
//     new_test_ext().execute_with(|| {
//         let sender = account_key(TEST_SENDER);
//         let id = TEST_PRODUCT_ID.as_bytes().to_owned();
//         let owner = account_key(TEST_ORGANIZATION);
//         let now = 42;
//         Timestamp::set_timestamp(now);

//         let result = ProductTracking::register_product(
//             Origin::signed(sender),
//             id.clone(),
//             owner.clone(),
//             None,
//         );

//         assert_ok!(result);

//         assert_eq!(
//             ProductTracking::product_by_id(&id),
//             Some(Product {
//                 id: id.clone(),
//                 owner: owner,
//                 registered: now,
//                 props: None
//             })
//         );

//         assert_eq!(ProductTracking::owner_of(&id), Some(owner));
//     });
// }

#[test]
fn create_shipment_with_valid_products() {
    new_test_ext().execute_with(|| {
        let sender = account_key(TEST_SENDER);
        let id = TEST_SHIPMENT_ID.as_bytes().to_owned();
        let owner = account_key(TEST_ORGANIZATION);
        let now = 42;
        Timestamp::set_timestamp(now);

        let result = ProductTracking::register_shipment(
            Origin::signed(sender),
            id.clone(),
            owner.clone(),
            vec![
                b"00012345600001".to_vec(),
                b"00012345600002".to_vec(),
                b"00012345600003".to_vec(),
            ],
        );

        assert_ok!(result);

        assert_eq!(
            ProductTracking::shipment_by_id(&id),
            Some(Shipment {
                id: id.clone(),
                owner: owner,
                registered: now,
                products: vec![
                    b"00012345600001".to_vec(),
                    b"00012345600002".to_vec(),
                    b"00012345600003".to_vec(),
                ],
            })
        );
    });
}

#[test]
fn create_shipment_with_invalid_sender() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ProductTracking::register_shipment(
                Origin::NONE,
                TEST_SHIPMENT_ID.as_bytes().to_owned(),
                account_key(TEST_ORGANIZATION),
                vec!()
            ),
            dispatch::DispatchError::BadOrigin
        );
    });
}

#[test]
fn create_shipment_with_missing_id() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ProductTracking::register_shipment(
                Origin::signed(account_key(TEST_SENDER)),
                vec!(),
                account_key(TEST_ORGANIZATION),
                vec!()
            ),
            Error::<Test>::ShipmentIdMissing
        );
    });
}

#[test]
fn create_shipment_with_long_id() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ProductTracking::register_shipment(
                Origin::signed(account_key(TEST_SENDER)),
                LONG_VALUE.as_bytes().to_owned(),
                account_key(TEST_ORGANIZATION),
                vec!()
            ),
            Error::<Test>::ShipmentIdTooLong
        );
    })
}

#[test]
fn create_shipment_with_existing_id() {
    new_test_ext().execute_with(|| {
        let existing_shipment = TEST_SHIPMENT_ID.as_bytes().to_owned();
        let now = 42;

        store_test_shipment::<Test>(
            existing_shipment.clone(),
            account_key(TEST_ORGANIZATION),
            now,
        );

        assert_noop!(
            ProductTracking::register_shipment(
                Origin::signed(account_key(TEST_SENDER)),
                existing_shipment,
                account_key(TEST_ORGANIZATION),
                vec![]
            ),
            Error::<Test>::ShipmentIdExists
        );
    })
}

#[test]
fn create_shipment_with_too_many_products() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ProductTracking::register_shipment(
                Origin::signed(account_key(TEST_SENDER)),
                TEST_SHIPMENT_ID.as_bytes().to_owned(),
                account_key(TEST_ORGANIZATION),
                vec![
                    b"00012345600001".to_vec(),
                    b"00012345600002".to_vec(),
                    b"00012345600003".to_vec(),
                    b"00012345600004".to_vec(),
                    b"00012345600005".to_vec(),
                    b"00012345600006".to_vec(),
                    b"00012345600007".to_vec(),
                    b"00012345600008".to_vec(),
                    b"00012345600009".to_vec(),
                    b"00012345600010".to_vec(),
                    b"00012345600011".to_vec(),
                ]
            ),
            Error::<Test>::ShipmentTooManyProducts
        );
    })
}

// #[test]
// fn create_product_with_invalid_prop_name() {
//     new_test_ext().execute_with(|| {
//         assert_noop!(
//             ProductTracking::register_product(
//                 Origin::signed(account_key(TEST_SENDER)),
//                 TEST_PRODUCT_ID.as_bytes().to_owned(),
//                 account_key(TEST_ORGANIZATION),
//                 Some(vec![
//                     ProductProperty::new(b"prop1", b"val1"),
//                     ProductProperty::new(b"prop2", b"val2"),
//                     ProductProperty::new(&LONG_VALUE.as_bytes().to_owned(), b"val3"),
//                 ])
//             ),
//             Error::<Test>::ProductInvalidPropName
//         );
//     })
// }

// #[test]
// fn create_product_with_invalid_prop_value() {
//     new_test_ext().execute_with(|| {
//         assert_noop!(
//             ProductTracking::register_product(
//                 Origin::signed(account_key(TEST_SENDER)),
//                 TEST_PRODUCT_ID.as_bytes().to_owned(),
//                 account_key(TEST_ORGANIZATION),
//                 Some(vec![
//                     ProductProperty::new(b"prop1", b"val1"),
//                     ProductProperty::new(b"prop2", b"val2"),
//                     ProductProperty::new(b"prop3", &LONG_VALUE.as_bytes().to_owned()),
//                 ])
//             ),
//             Error::<Test>::ProductInvalidPropValue
//         );
//     })
// }
