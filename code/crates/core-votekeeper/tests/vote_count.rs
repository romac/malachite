#![allow(clippy::bool_assert_comparison)]

use malachitebft_core_types::NilOrVal;

use arc_malachitebft_core_votekeeper::count::VoteCount;
use arc_malachitebft_core_votekeeper::{Threshold, ThresholdParam};

#[test]
fn vote_count_nil() {
    let t = 4;
    let q = ThresholdParam::TWO_F_PLUS_ONE;

    let mut vc = VoteCount::<u8, u32>::new();

    let addr1 = 1;
    let addr2 = 2;
    let addr3 = 3;
    let addr4 = 4;

    let val1 = 1;
    let val2 = 2;

    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 0);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    assert_eq!(vc.add(&addr1, &NilOrVal::Nil, 1), 1);
    assert_eq!(vc.get(&NilOrVal::Nil), 1);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 0);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    assert_eq!(vc.add(&addr2, &NilOrVal::Nil, 1), 2);
    assert_eq!(vc.get(&NilOrVal::Nil), 2);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 0);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    // addr1 votes again, is ignored
    assert_eq!(vc.add(&addr1, &NilOrVal::Nil, 1), 2);
    assert_eq!(vc.get(&NilOrVal::Nil), 2);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 0);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    assert_eq!(vc.add(&addr3, &NilOrVal::Nil, 1), 3);
    assert_eq!(vc.get(&NilOrVal::Nil), 3);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 0);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    assert_eq!(vc.add(&addr4, &NilOrVal::Val(1), 1), 1);
    assert_eq!(vc.get(&NilOrVal::Nil), 3);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 1);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);
}

#[test]
fn vote_count_value() {
    let t = 4;
    let q = ThresholdParam::TWO_F_PLUS_ONE;

    let mut vc = VoteCount::<u8, u32>::new();

    let addr1 = 1;
    let addr2 = 2;
    let addr3 = 3;
    let addr4 = 4;

    let val1 = 1;
    let val2 = 2;
    let val3 = 3;

    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 0);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    assert_eq!(vc.add(&addr1, &NilOrVal::Val(val1), 1), 1);
    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 1);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    assert_eq!(vc.add(&addr2, &NilOrVal::Val(val1), 1), 2);
    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 2);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    // addr1 votes again, for nil this time, is ignored
    assert_eq!(vc.add(&addr1, &NilOrVal::Nil, 1), 0);
    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 2);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    assert_eq!(vc.add(&addr3, &NilOrVal::Val(val1), 1), 3);
    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 3);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    // addr2 votes again, for the same value, is ignored
    assert_eq!(vc.add(&addr2, &NilOrVal::Val(val1), 1), 3);
    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 3);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    assert_eq!(vc.add(&addr4, &NilOrVal::Val(val2), 1), 1);
    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 3);
    assert_eq!(vc.get(&NilOrVal::Val(val2)), 1);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);

    // addr4 votes again, for a different value, is ignored
    assert_eq!(vc.add(&addr4, &NilOrVal::Val(val3), 1), 0);
    assert_eq!(vc.get(&NilOrVal::Nil), 0);
    assert_eq!(vc.get(&NilOrVal::Val(val1)), 3);
    assert_eq!(vc.get(&NilOrVal::Val(val2)), 1);
    assert_eq!(vc.get(&NilOrVal::Val(val3)), 0);
    assert_eq!(vc.is_threshold_met(Threshold::Unreached, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Any, q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Nil, q, t), false);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val1), q, t), true);
    assert_eq!(vc.is_threshold_met(Threshold::Value(val2), q, t), false);
}
