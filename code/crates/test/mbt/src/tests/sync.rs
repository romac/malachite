use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use itf::de::{As, Integer, Same};
use malachitebft_core_types::{ExtendedCommitCertificate, Round};
use malachitebft_sync as sync;
use malachitebft_test::{Height, TestContext, ValueId};
use rand::{rngs::StdRng, SeedableRng};
use serde::Deserialize;
use sync::{Effect, HeightStartType, Input, OutboundRequestId, PeerId, Resume};

use super::{check_differences, replay_cases};

#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "tag", content = "value")]
enum Boundary {
    Initial,
    Internal,
    Started,
    #[serde(with = "As::<(Same, Integer)>")]
    StatusUpdate(String, u64),
    #[serde(with = "As::<(Integer, Same, Integer, Integer)>")]
    Response(u64, String, u64, u64),
    #[serde(with = "As::<Integer>")]
    Decided(u64),
    #[serde(with = "As::<Integer>")]
    Transient(u64),
    Disconnected(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Range {
    #[serde(with = "As::<Integer>")]
    lo: u64,
    #[serde(with = "As::<Integer>")]
    hi: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Entry {
    range: Range,
    peer: String,
    excluded: BTreeSet<String>,
    inflight: bool,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    #[serde(with = "As::<Integer>")]
    history_min: u64,
    #[serde(with = "As::<Integer>")]
    tip: u64,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Client {
    started: bool,
    #[serde(with = "As::<Integer>")]
    tip: u64,
    #[serde(with = "As::<Integer>")]
    consensus_height: u64,
    #[serde(with = "As::<Integer>")]
    sync_height: u64,
    #[serde(with = "As::<Integer>")]
    next_req_id: u64,
    #[serde(with = "As::<BTreeMap<Integer, Same>>")]
    pending: BTreeMap<u64, Entry>,
    known: BTreeMap<String, Status>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Frame {
    boundary: Boundary,
    #[serde(rename = "clientReplayTest::valuesync::client")]
    client: Client,
}

struct ClientReplay {
    state: sync::State<TestContext>,
    peers: BTreeMap<String, PeerId>,
    metrics: sync::Metrics,
    next_id: u64,
    // Effects observed during a single external input, including any immediate refills.
    sent: Vec<(u64, String, Range)>,
    delivered: Vec<(u64, String, Vec<u64>)>,
}

impl ClientReplay {
    fn new() -> Self {
        let peers = ["p1", "p2"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                let hash = multihash::Multihash::wrap(0x12, &[i as u8; 32]).unwrap();
                (name.to_owned(), PeerId::from_multihash(hash).unwrap())
            })
            .collect();
        let config = sync::Config::default()
            .with_batch_size(2)
            .with_parallel_requests(2);
        Self {
            state: sync::State::new(Box::new(StdRng::seed_from_u64(42)), config),
            peers,
            metrics: sync::Metrics::new(Duration::from_secs(10)),
            next_id: 0,
            sent: Vec::new(),
            delivered: Vec::new(),
        }
    }

    fn run(&mut self, input: Input<TestContext>) -> Result<(), sync::Error<TestContext>> {
        let peers = &self.peers;
        sync::process!(input: input, state: &mut self.state, metrics: &self.metrics, with: e => {
            let resume = match e {
                Effect::SendValueRequest(peer, request, _) => {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.sent.push((id, peer_name(peers, peer), Range {
                        lo: request.range.start().as_u64(), hi: request.range.end().as_u64(),
                    }));
                    Resume::ValueRequestId(Some(OutboundRequestId::new(id)))
                }
                Effect::ProcessValueResponse(peer, id, response, _) => {
                    self.delivered.push((id.to_string().parse().unwrap(), peer_name(peers, peer),
                        response.values.iter().map(|v| v.certificate.height.as_u64()).collect()));
                    Resume::default()
                }
                Effect::BroadcastStatus(..) | Effect::CancelValueRequest(..) => Resume::default(),
                other => panic!("unexpected client effect {other:?}"),
            };
            Ok::<_, std::convert::Infallible>(resume)
        })
    }

    fn apply(&mut self, event: &Boundary) {
        self.sent.clear();
        self.delivered.clear();
        let mut forwarded = Vec::new();
        match event {
            Boundary::Initial => {}
            Boundary::Started => self
                .run(Input::StartedHeight(Height::new(1), HeightStartType::Start))
                .unwrap(),
            Boundary::StatusUpdate(peer, h) => self
                .run(Input::Status(sync::Status {
                    peer_id: self.peers[peer],
                    tip_height: Height::new(*h),
                    history_min_height: Height::new(1),
                }))
                .unwrap(),
            Boundary::Response(id, peer, lo, count) => {
                let heights: Vec<_> = (*lo..lo + count).collect();
                // Sync forwards the bytes and certificate to consensus for verification.
                let values = heights
                    .iter()
                    .map(|&h| {
                        sync::RawDecidedValue::new(
                            h.to_be_bytes().to_vec().into(),
                            ExtendedCommitCertificate {
                                height: Height::new(h),
                                round: Round::ZERO,
                                value_id: ValueId::new(h),
                                commit_signatures: vec![],
                            },
                        )
                    })
                    .collect();
                self.run(Input::ValueResponse(
                    OutboundRequestId::new(id),
                    self.peers[peer],
                    Some(sync::ValueResponse::new(Height::new(*lo), values)),
                ))
                .unwrap();
                forwarded.push((*id, peer.clone(), heights));
            }
            Boundary::Decided(h) => {
                self.run(Input::Decided(Height::new(*h))).unwrap();
                self.run(Input::StartedHeight(
                    Height::new(h + 1),
                    HeightStartType::Start,
                ))
                .unwrap();
            }
            Boundary::Transient(h) => self
                .run(Input::LocalTransientError(Height::new(*h)))
                .unwrap(),
            Boundary::Disconnected(peer) => {
                self.run(Input::PeerDisconnected(self.peers[peer])).unwrap()
            }
            Boundary::Internal => unreachable!(),
        }
        assert_eq!(
            self.delivered, forwarded,
            "values forwarded to consensus for {event:?}"
        );
    }

    fn pending(&self) -> BTreeMap<u64, Entry> {
        self.state
            .pending_requests
            .iter()
            .map(|(id, entry)| {
                (
                    id.to_string().parse().unwrap(),
                    Entry {
                        range: Range {
                            lo: entry.range.start().as_u64(),
                            hi: entry.range.end().as_u64(),
                        },
                        peer: peer_name(&self.peers, entry.peer),
                        excluded: entry
                            .excluded_peers
                            .iter()
                            .map(|&p| peer_name(&self.peers, p))
                            .collect(),
                        inflight: entry.inflight,
                    },
                )
            })
            .collect()
    }
}

fn peer_name(peers: &BTreeMap<String, PeerId>, peer: PeerId) -> String {
    peers
        .iter()
        .find(|(_, id)| **id == peer)
        .expect("unknown peer")
        .0
        .clone()
}

#[test]
fn client_itf() {
    let mut differences = Vec::new();
    replay_cases(
        "synchronization/valuesync/client-model/clientReplayTest.qnt",
        |name, trace: &itf::Trace<Frame>| {
            let mut replay = ClientReplay::new();
            let mut previous_id = 0;
            let mut events = 0;
            for (i, state) in trace.states.iter().enumerate() {
                let frame = &state.value;
                if frame.boundary == Boundary::Internal
                    || (i > 0 && frame == &trace.states[i - 1].value)
                {
                    continue;
                }
                replay.apply(&frame.boundary);
                let client = &frame.client;
                assert_eq!(
                    replay.state.started, client.started,
                    "{name} {:?}",
                    frame.boundary
                );
                assert_eq!(replay.state.tip_height.as_u64(), client.tip);
                assert_eq!(
                    replay.state.consensus_height.as_u64(),
                    client.consensus_height
                );
                assert_eq!(replay.state.sync_height.as_u64(), client.sync_height);
                assert_eq!(replay.next_id, client.next_req_id);
                let known = replay
                    .state
                    .peers
                    .iter()
                    .map(|(p, s)| {
                        (
                            peer_name(&replay.peers, *p),
                            Status {
                                history_min: s.history_min_height.as_u64(),
                                tip: s.tip_height.as_u64(),
                            },
                        )
                    })
                    .collect();
                assert_eq!(client.known, known);
                let sent: Vec<_> = client
                    .pending
                    .range(previous_id..)
                    .map(|(id, e)| (*id, e.peer.clone(), e.range.clone()))
                    .collect();
                let actual = replay.pending();
                if sent != replay.sent || client.pending != actual {
                    differences.push(format!("{name} {:?}\n  sends: Quint {sent:?} / Rust {:?}\n  pending: Quint {:?} / Rust {actual:?}",
                    frame.boundary, replay.sent, client.pending));
                }
                previous_id = client.next_req_id;
                events += 1;
            }
            assert!(events > 1, "no sync inputs replayed for {name}");
        },
    );
    check_differences(include_str!("sync-known-differences.txt"), &differences);
}
