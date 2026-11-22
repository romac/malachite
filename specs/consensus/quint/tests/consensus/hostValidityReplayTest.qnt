module hostValidityReplayTest {
    import types.* from "../../types"
    import consensus.* from "../../consensus"

    // The round model takes a prevalidated proposal. Expose the two external
    // events that supply it at the core-consensus boundary.
    type Event = Start | Envelope | HostAccepted
    var event: Event
    var payload: str
    var hasEnvelope: bool
    var hostAccepted: bool
    var state: ConsensusState
    var votes: List[Vote]

    action start(mode: str): bool = all {
        event' = Start,
        payload' = mode,
        hasEnvelope' = false,
        hostAccepted' = false,
        state' = consensus(initConsensusState("local", 1), NewRoundCInput(0)).cs,
        votes' = [],
    }

    action deliver(e: Event): bool = {
        val envelope = hasEnvelope or e == Envelope
        val accepted = hostAccepted or e == HostAccepted
        val ready = envelope and accepted and not(hasEnvelope and hostAccepted)
        val result = if (ready)
            consensus(state, ProposalCInput((0, Val("block"))))
            else { cs: state, out: NoConsensusOutput }
        all {
            event' = e,
            payload' = payload,
            hasEnvelope' = envelope,
            hostAccepted' = accepted,
            state' = result.cs,
            votes' = match result.out { | VoteOutput(v) => [v] | _ => [] },
        }
    }

    run proposalOnlyPendingHostTest =
        start("ProposalOnly").then(deliver(Envelope))
        .expect(state.step == ProposeStep and votes == [])

    run partsEnvelopeFirstTest =
        start("ProposalAndParts").then(deliver(Envelope))
        .expect(state.step == ProposeStep and votes == [])
        .then(deliver(HostAccepted))
        .expect(state.step == PrevoteStep and votes.length() == 1)

    run partsHostFirstTest =
        start("ProposalAndParts").then(deliver(HostAccepted))
        .expect(state.step == ProposeStep and votes == [])
        .then(deliver(Envelope))
        .expect(state.step == PrevoteStep and votes.length() == 1)
}
