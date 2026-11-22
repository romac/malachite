use std::collections::HashMap;

use malachitebft_core_types::NilOrVal;
use malachitebft_core_votekeeper as core;
use malachitebft_test::{Address, TestContext, ValueId};

use crate::types::Value;

pub fn value_from_model(value: &Value) -> NilOrVal<ValueId> {
    match value {
        Value::Nil => NilOrVal::Nil,
        Value::Val(v) => NilOrVal::Val(value_id(v)),
    }
}

pub fn value_id(v: &str) -> ValueId {
    match v {
        "v1" => 1.into(),
        "v2" => 2.into(),
        "v3" => 3.into(),
        _ => panic!("unknown vote value {v:?}"),
    }
}

pub fn check_votes(
    expected: &crate::votekeeper::VoteCount,
    actual: &core::count::VoteCount<TestContext>,
    address_map: &HashMap<String, Address>,
) {
    let expected_values_weights = &expected.values_weights;
    let actual_values_weights = &actual.values_weights;

    assert_eq!(
        actual_values_weights.sum(),
        expected_values_weights.values().sum::<i64>() as u64
    );

    for value in expected_values_weights.keys() {
        assert_eq!(
            actual_values_weights.get(&value_from_model(value)),
            *expected_values_weights.get(value).unwrap() as u64,
            "weight for value {value:?}"
        );
    }

    let expected_votes_addresses = &expected.votes_addresses;
    let actual_votes_addresses = &actual.validator_addresses;

    assert_eq!(
        actual_votes_addresses.len(),
        expected_votes_addresses.len(),
        "number of voted addresses"
    );

    for address in expected_votes_addresses {
        assert!(
            actual_votes_addresses.contains(address_map.get(address).unwrap()),
            "address {address:?} not voted"
        );
    }
}
