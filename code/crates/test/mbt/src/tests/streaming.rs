use std::collections::BTreeSet;
use std::sync::{Arc, Weak};

use itf::de::{As, Integer};
use malachitebft_app_channel::app::consensus::PeerId;
use malachitebft_core_types::Round;
use malachitebft_engine::util::streaming::{StreamContent, StreamId, StreamMessage};
use malachitebft_test::{Address, Height, ProposalData, ProposalInit, ProposalPart};
use malachitebft_test_streaming::PartStreamsMap;
use serde::Deserialize;

use super::{check_differences, replay_cases};

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "tag")]
enum Kind {
    #[serde(rename = "INIT")]
    Init,
    #[serde(rename = "DATA")]
    Data,
    #[serde(rename = "FIN")]
    Fin,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    #[serde(with = "As::<Integer>")]
    sequence: u64,
    msg_type: Kind,
    payload: String,
}

impl Message {
    fn part(&self) -> Option<ProposalPart> {
        match (&self.msg_type, self.payload.as_str()) {
            (Kind::Init, "Init") => Some(ProposalPart::Init(ProposalInit::new(
                Height::new(1),
                Round::new(0),
                Round::Nil,
                Address::new([1; 20]),
            ))),
            (Kind::Data, data) => {
                let factor = data.strip_prefix("Data ").unwrap().parse().unwrap();
                Some(ProposalPart::Data(ProposalData::new(factor)))
            }
            (Kind::Fin, "Fin") => None,
            _ => panic!("unknown streaming fixture {self:?}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "tag", content = "value")]
enum Incoming {
    None,
    Some(Message),
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct StreamState {
    emitted: Vec<Message>,
    received: Vec<Message>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Frame {
    state: StreamState,
    incoming_message: Incoming,
}

#[test]
fn stream_delivery_orders() {
    replay_cases(
        "block-streaming/streamReplayTest.qnt",
        |case, trace: &itf::Trace<Frame>| {
            let [publisher, relay_one, relay_two] = [1, 2, 3].map(|n| {
                PeerId::from_multihash(multihash::Multihash::wrap(0x12, &[n; 32]).unwrap()).unwrap()
            });
            let relays = [relay_one, relay_two];
            let mut stream_id = None;
            let mut owners: Vec<Weak<[u8]>> = Vec::new();
            let mut map = PartStreamsMap::new();
            let mut seen_orders = BTreeSet::new();
            let mut order = Vec::new();

            for (i, state) in trace.states.iter().enumerate() {
                let frame = &state.value;
                let Incoming::Some(msg) = &frame.incoming_message else {
                    assert!(frame.state.received.is_empty());
                    assert!(
                        stream_id.is_none(),
                        "new schedule before previous stream completed"
                    );
                    order.clear();
                    continue;
                };
                // Quint expect() can emit a stuttering frame; each delivery adds one message.
                if frame.state.received.len() == order.len() {
                    assert_eq!(frame, &trace.states[i - 1].value);
                    continue;
                }
                order.push(msg.sequence);
                assert_eq!(order.len(), frame.state.received.len());
                let id = stream_id.get_or_insert_with(|| {
                    let owner: Arc<[u8]> =
                        (seen_orders.len() as u64).to_be_bytes().as_slice().into();
                    owners.push(Arc::downgrade(&owner));
                    StreamId::new(bytes::Bytes::from_owner(owner))
                });
                let content = msg.part().map_or(StreamContent::Fin, StreamContent::Data);
                let actual = map.insert(
                    relays[i % 2],
                    Some(publisher),
                    StreamMessage::new(id.clone(), msg.sequence, content),
                );
                let complete = frame
                    .state
                    .emitted
                    .last()
                    .is_some_and(|last| last.msg_type == Kind::Fin);
                assert_eq!(actual.is_some(), complete, "order {order:?}");
                if let Some(parts) = actual {
                    let expected: Vec<_> = frame
                        .state
                        .emitted
                        .iter()
                        .filter_map(Message::part)
                        .collect();
                    assert_eq!(parts.parts, expected, "order {order:?}");
                    assert_eq!(parts.height, Height::new(1));
                    assert_eq!(parts.round, Round::new(0));
                    assert_eq!(parts.proposer, Address::new([1; 20]));
                    assert_eq!(
                        order.iter().copied().collect::<BTreeSet<_>>(),
                        BTreeSet::from([0, 1, 2, 3])
                    );
                    assert!(
                        seen_orders.insert(order.clone()),
                        "duplicate permutation {order:?}"
                    );
                    drop(stream_id.take());
                }
            }
            assert_eq!(seen_orders.len(), 24);
            // A completed stream should release its ID while the map is still serving other streams.
            let retained = owners.iter().filter(|id| id.strong_count() != 0).count();
            drop(map);
            assert!(
                owners.iter().all(|id| id.strong_count() == 0),
                "fixture retained a stream ID"
            );
            let differences = (retained != 0).then(|| {
                format!("{case}: completed stream IDs retained: expected 0 / Rust {retained}")
            });
            check_differences(
                "allDeliveryOrdersTest: completed stream IDs retained: expected 0 / Rust 24",
                differences.as_slice(),
            );
        },
    );
}
