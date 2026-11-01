//! A deterministic discrete-event simulation of a network of consensus nodes.
//!
//! The simulator plays the roles of the network, the timers and the application:
//! every [`Effect`] yielded by a node's consensus state is turned into a scheduled
//! event (a message delivery, a timeout, a value to propose, a new height), and
//! events are processed in virtual-time order. Given the same [`SimConfig`], a run
//! always produces the same sequence of [`SimEvent`]s.

use std::collections::{BTreeMap, HashMap};

use bytes::Bytes;
use malachitebft_core_consensus::{
    process, Effect, Error, Input, LivenessMsg, LocallyProposedValue, Params, PeerId,
    ProposedValue, Resumable, Resume, SignedConsensusMsg, State,
};
use malachitebft_core_types::{
    ExtendedCommitCertificate, LinearTimeouts, NilOrVal, PolkaCertificate, Round, RoundCertificate,
    SignedMessage, SignedVote, Timeout, TimeoutKind, Validity, ValueOrigin, ValuePayload,
    ValueResponse, Vote as _, VoteType,
};
use serde::Serialize;

use crate::context::{Address, Height, SimContext, ValidatorSet, Value};

/// Configuration of a simulation run.
#[derive(Clone, Debug)]
pub struct SimConfig {
    /// Number of validators, all with equal voting power.
    pub num_nodes: u32,
    /// Seed for the pseudo-random number generator (message delays and drops).
    pub seed: u64,
    /// Minimum one-way message delay, in milliseconds.
    pub min_delay_ms: u64,
    /// Maximum one-way message delay, in milliseconds.
    pub max_delay_ms: u64,
    /// Probability in `[0, 1]` that a message is dropped.
    pub drop_rate: f64,
    /// Consensus timeouts.
    pub timeouts: LinearTimeouts,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            num_nodes: 4,
            seed: 0,
            min_delay_ms: 20,
            max_delay_ms: 200,
            drop_rate: 0.0,
            timeouts: LinearTimeouts::default(),
        }
    }
}

/// Kind of a message exchanged between nodes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub enum MsgKind {
    Proposal,
    Prevote,
    Precommit,
    PolkaCertificate,
    RoundCertificate,
    SyncRequest,
    SyncResponse,
}

/// Something observable that happened during the simulation, for visualization.
///
/// All times are in virtual milliseconds since the start of the simulation.
/// Rounds are `-1` when nil.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum SimEvent {
    /// A node started a new round.
    RoundStarted {
        time: u64,
        node: u32,
        height: u64,
        round: i64,
        proposer: u32,
    },
    /// A node moved to a new step of the consensus state machine.
    StepChanged {
        time: u64,
        node: u32,
        height: u64,
        round: i64,
        step: String,
    },
    /// A message was sent from one node to another.
    /// It arrives at `deliver_at`, unless `dropped` is set.
    MessageSent {
        time: u64,
        from: u32,
        to: u32,
        deliver_at: u64,
        dropped: bool,
        kind: MsgKind,
        height: u64,
        round: i64,
        /// The proposed or voted-for value, `None` for a nil vote or a certificate.
        value: Option<u64>,
    },
    /// A timeout elapsed on a node.
    TimeoutElapsed {
        time: u64,
        node: u32,
        kind: String,
        round: i64,
    },
    /// A node decided on a value.
    Decided {
        time: u64,
        node: u32,
        height: u64,
        round: i64,
        value: u64,
    },
}

/// Current status of a node.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NodeStatus {
    pub node: u32,
    pub online: bool,
    pub height: u64,
    pub round: i64,
    pub step: String,
    /// Decided values, indexed by height minus one.
    pub decisions: Vec<u64>,
}

#[derive(Clone, Debug)]
enum NetMsg {
    Vote(SignedVote<SimContext>),
    Proposal(SignedMessage<SimContext, crate::context::Proposal>),
    PolkaCertificate(PolkaCertificate<SimContext>),
    RoundCertificate(RoundCertificate<SimContext>),
}

impl NetMsg {
    fn into_input(self) -> Input<SimContext> {
        match self {
            NetMsg::Vote(v) => Input::Vote(v),
            NetMsg::Proposal(p) => Input::Proposal(p),
            NetMsg::PolkaCertificate(c) => Input::PolkaCertificate(c),
            NetMsg::RoundCertificate(c) => Input::RoundCertificate(c),
        }
    }

    fn height(&self) -> Height {
        match self {
            NetMsg::Vote(v) => v.height,
            NetMsg::Proposal(p) => p.height,
            NetMsg::PolkaCertificate(c) => c.height,
            NetMsg::RoundCertificate(c) => c.height,
        }
    }

    /// Kind, height, round and value, for reporting.
    fn describe(&self) -> (MsgKind, u64, i64, Option<u64>) {
        match self {
            NetMsg::Vote(v) => {
                let kind = match v.vote_type() {
                    VoteType::Prevote => MsgKind::Prevote,
                    VoteType::Precommit => MsgKind::Precommit,
                };
                let value = match v.value() {
                    NilOrVal::Nil => None,
                    NilOrVal::Val(id) => Some(*id),
                };
                (kind, v.height.0, v.round.as_i64(), value)
            }
            NetMsg::Proposal(p) => (
                MsgKind::Proposal,
                p.height.0,
                p.round.as_i64(),
                Some(p.value.0),
            ),
            NetMsg::PolkaCertificate(c) => (
                MsgKind::PolkaCertificate,
                c.height.0,
                c.round.as_i64(),
                Some(c.value_id),
            ),
            NetMsg::RoundCertificate(c) => (
                MsgKind::RoundCertificate,
                c.height.0,
                c.round.as_i64(),
                None,
            ),
        }
    }
}

/// A scheduled event, processed at its virtual time.
#[derive(Debug)]
enum Event {
    StartHeight {
        node: u32,
        height: Height,
    },
    Deliver {
        from: u32,
        to: u32,
        msg: NetMsg,
    },
    /// `from` asks `to` for the decided value and certificate at `height`.
    SyncRequest {
        from: u32,
        to: u32,
        height: Height,
    },
    /// Response to a [`Event::SyncRequest`], `None` if the peer has not decided that height.
    SyncResponse {
        to: u32,
        response: Option<ValueResponse<SimContext>>,
    },
    Timeout {
        node: u32,
        timeout: Timeout,
        generation: u64,
    },
    Propose {
        node: u32,
        height: Height,
        round: Round,
    },
}

/// What a node asked for while processing an input, applied after processing completes.
enum Action {
    Broadcast(NetMsg),
    ScheduleTimeout(Timeout),
    CancelTimeout(Timeout),
    CancelAllTimeouts,
    GetValue(Height, Round),
    RoundStarted(Height, Round, Address),
    Decided(Height, Round, Value),
    Finalized(ExtendedCommitCertificate<SimContext>),
    SyncedValue(ProposedValue<SimContext>),
}

struct Node {
    state: State<SimContext>,
    online: bool,
    /// Active timeouts, mapped to the generation of their latest scheduling.
    timers: HashMap<Timeout, u64>,
    decisions: Vec<u64>,
    /// Certificates for decided heights, served to lagging peers.
    certificates: BTreeMap<u64, ExtendedCommitCertificate<SimContext>>,
    /// Whether a sync request is in flight.
    syncing: bool,
    /// Last observed `(height, round, step)`, to report step changes.
    last_seen: (u64, i64, String),
}

/// A deterministic simulation of `num_nodes` validators running consensus.
pub struct Simulation {
    config: SimConfig,
    validator_set: ValidatorSet,
    nodes: Vec<Node>,
    now: u64,
    seq: u64,
    queue: BTreeMap<(u64, u64), Event>,
    rng: SplitMix64,
    next_generation: u64,
    events: Vec<SimEvent>,
}

impl Simulation {
    pub fn new(config: SimConfig) -> Self {
        assert!(config.num_nodes > 0, "at least one node is required");
        assert!(config.min_delay_ms <= config.max_delay_ms);

        let validator_set = ValidatorSet::uniform(config.num_nodes);

        let nodes = (0..config.num_nodes)
            .map(|i| Node {
                state: State::new(
                    SimContext,
                    Height(1),
                    validator_set.clone(),
                    Params {
                        address: Address(i),
                        threshold_params: Default::default(),
                        value_payload: ValuePayload::ProposalOnly,
                        enabled: true,
                    },
                    1000,
                    1000,
                ),
                online: true,
                timers: HashMap::new(),
                decisions: Vec::new(),
                certificates: BTreeMap::new(),
                syncing: false,
                last_seen: (0, -1, String::new()),
            })
            .collect();

        let mut sim = Self {
            rng: SplitMix64(config.seed),
            config,
            validator_set,
            nodes,
            now: 0,
            seq: 0,
            queue: BTreeMap::new(),
            next_generation: 0,
            events: Vec::new(),
        };

        for node in 0..sim.config.num_nodes {
            sim.schedule(
                0,
                Event::StartHeight {
                    node,
                    height: Height(1),
                },
            );
        }

        sim
    }

    /// Current virtual time, in milliseconds.
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Time of the next pending event, if any.
    pub fn next_event_time(&self) -> Option<u64> {
        self.queue.keys().next().map(|(t, _)| *t)
    }

    /// Process the next pending event. Returns `false` if there are none.
    pub fn step(&mut self) -> bool {
        let Some(((time, _), event)) = self.queue.pop_first() else {
            return false;
        };

        self.now = time;
        self.handle_event(event);
        true
    }

    /// Process all events scheduled up to and including `time`, then advance the clock to `time`.
    pub fn run_until(&mut self, time: u64) {
        while self.next_event_time().is_some_and(|t| t <= time) {
            self.step();
        }
        self.now = self.now.max(time);
    }

    /// Take all events observed since the last call.
    pub fn drain_events(&mut self) -> Vec<SimEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn statuses(&self) -> Vec<NodeStatus> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, n)| NodeStatus {
                node: i as u32,
                online: n.online,
                height: n.state.height().0,
                round: n.state.round().as_i64(),
                step: format!("{:?}", n.state.driver.step()),
                decisions: n.decisions.clone(),
            })
            .collect()
    }

    /// Take a node offline or bring it back online.
    ///
    /// An offline node drops every message and timeout addressed to it.
    /// When it comes back, it catches up with its peers using sync.
    pub fn set_online(&mut self, node: u32, online: bool) {
        let n = &mut self.nodes[node as usize];
        if n.online == online {
            return;
        }
        n.online = online;

        // A node coming back restarts its timers for the current round,
        // so that it does not stay stuck waiting for a timeout it missed.
        if online {
            let round = n.state.round();
            if round.is_defined() {
                for timeout in [Timeout::propose(round), Timeout::rebroadcast(round)] {
                    self.schedule_timeout(node, timeout);
                }
            }
        }
    }

    pub fn set_drop_rate(&mut self, drop_rate: f64) {
        self.config.drop_rate = drop_rate.clamp(0.0, 1.0);
    }

    pub fn set_delay_ms(&mut self, min: u64, max: u64) {
        self.config.min_delay_ms = min.min(max);
        self.config.max_delay_ms = max.max(min);
    }

    fn schedule(&mut self, time: u64, event: Event) {
        self.seq += 1;
        self.queue.insert((time, self.seq), event);
    }

    fn schedule_timeout(&mut self, node: u32, timeout: Timeout) {
        let generation = self.next_generation;
        self.next_generation += 1;
        self.nodes[node as usize].timers.insert(timeout, generation);

        let duration = match timeout.kind {
            TimeoutKind::FinalizeHeight(d) => d,
            _ => self.config.timeouts.duration_for(timeout),
        };
        self.schedule(
            self.now + duration.as_millis() as u64,
            Event::Timeout {
                node,
                timeout,
                generation,
            },
        );
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::StartHeight { node, height } => {
                let vs = self.validator_set.clone();
                self.process(
                    node,
                    Input::StartHeight(height, vs, false, None, Default::default()),
                );
            }

            Event::Deliver { from, to, msg } => {
                let n = &mut self.nodes[to as usize];
                if !n.online {
                    return;
                }

                // A message from a higher height means the sender has decided our
                // current height: ask it for the decided value, like ValueSync does.
                let height = n.state.height();
                if msg.height() > height && !n.syncing {
                    n.syncing = true;
                    self.send_sync(
                        to,
                        from,
                        MsgKind::SyncRequest,
                        height.0,
                        None,
                        Event::SyncRequest {
                            from: to,
                            to: from,
                            height,
                        },
                    );
                }

                self.process(to, msg.into_input());
            }

            Event::SyncRequest { from, to, height } => {
                let server = &self.nodes[to as usize];
                let response = server
                    .online
                    .then(|| server.certificates.get(&height.0))
                    .flatten()
                    .map(|certificate| {
                        ValueResponse::new(
                            sim_peer_id(to),
                            // Value ids are the values themselves.
                            Bytes::copy_from_slice(&certificate.value_id.to_be_bytes()),
                            certificate.clone(),
                        )
                    });

                let value = response.as_ref().map(|r| r.certificate.value_id);
                self.send_sync(
                    to,
                    from,
                    MsgKind::SyncResponse,
                    height.0,
                    value,
                    Event::SyncResponse { to: from, response },
                );
            }

            Event::SyncResponse { to, response } => {
                let n = &mut self.nodes[to as usize];
                n.syncing = false;
                if let (true, Some(response)) = (n.online, response) {
                    self.process(to, Input::SyncValueResponse(response));
                }
            }

            Event::Timeout {
                node,
                timeout,
                generation,
            } => {
                let n = &mut self.nodes[node as usize];
                if n.timers.get(&timeout) != Some(&generation) {
                    return; // Cancelled or rescheduled
                }
                n.timers.remove(&timeout);

                if !n.online {
                    return;
                }

                self.events.push(SimEvent::TimeoutElapsed {
                    time: self.now,
                    node,
                    kind: timeout_kind_name(timeout.kind).to_string(),
                    round: timeout.round.as_i64(),
                });
                self.process(node, Input::TimeoutElapsed(timeout));
            }

            Event::Propose {
                node,
                height,
                round,
            } => {
                let n = &self.nodes[node as usize];
                if !n.online || n.state.height() != height || n.state.round() != round {
                    return;
                }
                let value = Value(height.0 * 100 + u64::from(node));
                self.process(
                    node,
                    Input::Propose(LocallyProposedValue::new(height, round, value)),
                );
            }
        }
    }

    /// Feed an input to a node's consensus state, then apply the resulting actions.
    fn process(&mut self, node: u32, input: Input<SimContext>) {
        let mut actions = Vec::new();
        // `()` when the `metrics` feature of core-consensus is off (eg. in wasm builds),
        // but a real `Metrics` when feature unification enables it in workspace builds.
        #[allow(clippy::let_unit_value)]
        let metrics = Default::default();

        let n = &mut self.nodes[node as usize];
        // Consensus may reject an input (eg. a message for a past height): that is not fatal.
        let _: Result<(), Error<SimContext>> = process!(
            input: input,
            state: &mut n.state,
            metrics: &metrics,
            with: effect => handle_effect(effect, &mut actions)
        );

        for action in actions {
            self.apply(node, action);
        }

        self.report_step(node);
    }

    fn apply(&mut self, node: u32, action: Action) {
        match action {
            Action::Broadcast(msg) => self.broadcast(node, msg),
            Action::ScheduleTimeout(timeout) => self.schedule_timeout(node, timeout),
            Action::CancelTimeout(timeout) => {
                self.nodes[node as usize].timers.remove(&timeout);
            }
            Action::CancelAllTimeouts => self.nodes[node as usize].timers.clear(),
            Action::GetValue(height, round) => {
                // Building a value takes a little while.
                self.schedule(
                    self.now + 10,
                    Event::Propose {
                        node,
                        height,
                        round,
                    },
                );
            }
            Action::RoundStarted(height, round, proposer) => {
                self.events.push(SimEvent::RoundStarted {
                    time: self.now,
                    node,
                    height: height.0,
                    round: round.as_i64(),
                    proposer: proposer.0,
                });
            }
            Action::Decided(height, round, value) => {
                self.nodes[node as usize].decisions.push(value.0);
                self.events.push(SimEvent::Decided {
                    time: self.now,
                    node,
                    height: height.0,
                    round: round.as_i64(),
                    value: value.0,
                });
            }
            Action::Finalized(certificate) => {
                let height = certificate.height;
                self.nodes[node as usize]
                    .certificates
                    .insert(height.0, certificate);
                self.schedule(
                    self.now,
                    Event::StartHeight {
                        node,
                        height: Height(height.0 + 1),
                    },
                );
            }
            Action::SyncedValue(value) => {
                self.process(node, Input::ProposedValue(value, ValueOrigin::Sync));
            }
        }
    }

    /// Send a sync request or response. These go over a reliable channel and are never dropped.
    fn send_sync(
        &mut self,
        from: u32,
        to: u32,
        kind: MsgKind,
        height: u64,
        value: Option<u64>,
        event: Event,
    ) {
        let delay = self
            .rng
            .range(self.config.min_delay_ms, self.config.max_delay_ms);
        let deliver_at = self.now + delay;

        self.events.push(SimEvent::MessageSent {
            time: self.now,
            from,
            to,
            deliver_at,
            dropped: false,
            kind,
            height,
            round: -1,
            value,
        });

        self.schedule(deliver_at, event);
    }

    fn broadcast(&mut self, from: u32, msg: NetMsg) {
        let (kind, height, round, value) = msg.describe();

        for to in 0..self.config.num_nodes {
            if to == from {
                continue; // Consensus applies its own messages internally
            }

            let dropped = self.rng.next_f64() < self.config.drop_rate;
            let delay = self
                .rng
                .range(self.config.min_delay_ms, self.config.max_delay_ms);
            let deliver_at = self.now + delay;

            self.events.push(SimEvent::MessageSent {
                time: self.now,
                from,
                to,
                deliver_at,
                dropped,
                kind,
                height,
                round,
                value,
            });

            if !dropped {
                self.schedule(
                    deliver_at,
                    Event::Deliver {
                        from,
                        to,
                        msg: msg.clone(),
                    },
                );
            }
        }
    }

    fn report_step(&mut self, node: u32) {
        let n = &mut self.nodes[node as usize];
        let current = (
            n.state.height().0,
            n.state.round().as_i64(),
            format!("{:?}", n.state.driver.step()),
        );

        if current != n.last_seen {
            self.events.push(SimEvent::StepChanged {
                time: self.now,
                node,
                height: current.0,
                round: current.1,
                step: current.2.clone(),
            });
            n.last_seen = current;
        }
    }
}

/// Handle an effect synchronously, deferring anything that touches the
/// simulation (network, timers, scheduling) to `actions`.
fn handle_effect(
    effect: Effect<SimContext>,
    actions: &mut Vec<Action>,
) -> Result<Resume<SimContext>, ()> {
    let resume = match effect {
        Effect::CancelAllTimeouts(r) => {
            actions.push(Action::CancelAllTimeouts);
            r.resume_with(())
        }
        Effect::CancelTimeout(timeout, r) => {
            actions.push(Action::CancelTimeout(timeout));
            r.resume_with(())
        }
        Effect::ScheduleTimeout(timeout, r) => {
            actions.push(Action::ScheduleTimeout(timeout));
            r.resume_with(())
        }
        Effect::StartRound(height, round, proposer, _role, r) => {
            actions.push(Action::RoundStarted(height, round, proposer));
            r.resume_with(())
        }
        Effect::PublishConsensusMsg(msg, r) => {
            actions.push(Action::Broadcast(match msg {
                SignedConsensusMsg::Vote(v) => NetMsg::Vote(v),
                SignedConsensusMsg::Proposal(p) => NetMsg::Proposal(p),
            }));
            r.resume_with(())
        }
        Effect::PublishLivenessMsg(msg, r) => {
            actions.push(Action::Broadcast(match msg {
                LivenessMsg::Vote(v) => NetMsg::Vote(v),
                LivenessMsg::PolkaCertificate(c) => NetMsg::PolkaCertificate(c),
                LivenessMsg::SkipRoundCertificate(c) => NetMsg::RoundCertificate(c),
            }));
            r.resume_with(())
        }
        Effect::RepublishVote(v, r) => {
            actions.push(Action::Broadcast(NetMsg::Vote(v)));
            r.resume_with(())
        }
        Effect::RepublishRoundCertificate(c, r) => {
            actions.push(Action::Broadcast(NetMsg::RoundCertificate(c)));
            r.resume_with(())
        }
        Effect::GetValue(height, round, _timeout, r) => {
            actions.push(Action::GetValue(height, round));
            r.resume_with(())
        }
        // In `ProposalOnly` mode, the proposal message carries the full value.
        Effect::RestreamProposal(.., r) => r.resume_with(()),
        Effect::CertVerifiedSyncValue(response, proposer, r) => {
            if let Ok(bytes) = <[u8; 8]>::try_from(response.value_bytes.as_ref()) {
                actions.push(Action::SyncedValue(ProposedValue {
                    height: response.certificate.height,
                    round: response.certificate.round,
                    valid_round: Round::Nil,
                    proposer,
                    value: Value(u64::from_be_bytes(bytes)),
                    validity: Validity::Valid,
                }));
            }
            r.resume_with(())
        }
        Effect::CertRejectedSyncValue(_, _, _, r) => r.resume_with(()),
        Effect::Decide(certificate, _extensions, r) => {
            actions.push(Action::Decided(
                certificate.height,
                certificate.round,
                Value(certificate.value_id),
            ));
            r.resume_with(())
        }
        Effect::Finalize(certificate, extensions, _evidence, r) => {
            actions.push(Action::Finalized(
                ExtendedCommitCertificate::from_commit_certificate_and_extensions(
                    certificate,
                    extensions,
                ),
            ));
            r.resume_with(())
        }
        Effect::SignVote(vote, r) => {
            let signer = vote.voter;
            r.resume_with(SignedMessage::new(vote, signer))
        }
        Effect::SignProposal(proposal, r) => {
            let signer = proposal.proposer;
            r.resume_with(SignedMessage::new(proposal, signer))
        }
        Effect::VerifySignature(msg, public_key, r) => r.resume_with(msg.signature == public_key),
        // Certificates are made of signatures which are trivially valid here.
        Effect::VerifyCommitCertificate(.., r) => r.resume_with(Ok(())),
        Effect::VerifyExtendedCommitCertificate(.., r) => r.resume_with(Ok(())),
        Effect::VerifyPolkaCertificate(.., r) => r.resume_with(Ok(())),
        Effect::VerifyRoundCertificate(.., r) => r.resume_with(Ok(())),
        // Nothing to persist: nodes never crash and restart from disk.
        Effect::WalAppend(_, _, r) => r.resume_with(()),
        // Vote extensions are disabled.
        Effect::ExtendVote(.., r) => r.resume_with(None),
        Effect::VerifyVoteExtension(.., r) => r.resume_with(Ok(())),
    };

    Ok(resume)
}

/// A peer id for the given node: an identity multihash of its index.
fn sim_peer_id(node: u32) -> PeerId {
    let mut bytes = vec![0x00, 4]; // identity multihash code, digest length
    bytes.extend_from_slice(&node.to_be_bytes());
    PeerId::from_bytes(&bytes).expect("valid identity multihash")
}

fn timeout_kind_name(kind: TimeoutKind) -> &'static str {
    match kind {
        TimeoutKind::Propose => "Propose",
        TimeoutKind::Prevote => "Prevote",
        TimeoutKind::Precommit => "Precommit",
        TimeoutKind::Rebroadcast => "Rebroadcast",
        TimeoutKind::FinalizeHeight(_) => "FinalizeHeight",
    }
}

/// Small, fast, deterministic PRNG, to avoid depending on `rand` and `getrandom`.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[min, max]`.
    fn range(&mut self, min: u64, max: u64) -> u64 {
        min + self.next_u64() % (max - min + 1)
    }
}
