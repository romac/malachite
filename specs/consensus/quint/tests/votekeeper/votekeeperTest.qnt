// -*- mode: Bluespec; -*-

module votekeeperTest {

    import types.* from "../../types"
    import votekeeper.* from "../../votekeeper"

    // ****************************************************************************
    // Tests
    // ****************************************************************************

    // auxiliary action for tests
    action _assert(predicate: bool): bool = 
        all { assert(predicate), allUnchanged }


    // Consensus full execution with all honest validators (including the leader) and a synchronous network:
    // all messages are received in order. We assume three validators in the validator set with 60%, 30% and 10%
    // each of the total voting power
    run synchronousConsensusTest =
        initWith(1, Map("alice" -> 60, "bob" -> 30, "john" -> 10))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == PolkaValueVKOutput((1, "v1")))
        .then(applyVoteAction(mkVote(Prevote, "bob", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "bob", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "john", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == PrecommitValueVKOutput((1, "v1")))

    // Reaching PolkaAny 
    run polkaAnyTest =
        initWith(1, Map("alice" -> 60, "bob" -> 30, "john" -> 10))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 1, Nil), 1))
        .expect(lastEmitted == PolkaAnyVKOutput(1))

    // Reaching PolkaNil
    run polkaNilTest =
        initWith(1, Map("alice" -> 60, "bob" -> 30, "john" -> 10))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Nil), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 1, Nil), 1))
        .expect(lastEmitted == PolkaNilVKOutput(1))

    // Reaching Skip via n+1 threshold with prevotes from two validators at a future round
    run skipSmallQuorumAllPrevotesTest =
        initWith(1, Map("alice" -> 60, "bob" -> 30, "john" -> 10))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "bob", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == SkipVKOutput(2))

    // Cannot reach Skip via f+1 threshold with one prevote and one precommit from the same validator at a future round
    run noSkipSmallQuorumMixedVotesSameValTest =
        initWith(1, Map("alice" -> 10, "bob" -> 60, "john" -> 20))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted != SkipVKOutput(2))

    // Reaching Skip via f+1 threshold with one prevote and one precommit from two validators at a future round
    run skipSmallQuorumMixedVotesTwoValsTest =
        initWith(1, Map("alice" -> 50, "bob" -> 20, "john" -> 10))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "bob", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == SkipVKOutput(2))
        
    // Reaching Skip via 2f+1 threshold with a single prevote from a single validator at a future round
    run skipQuorumSinglePrevoteTest =
        initWith(1, Map("alice" -> 10, "bob" -> 30, "john" -> 60))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == SkipVKOutput(2))

    // Reaching Skip via 2f+1 threshold with a single precommit from a single validator at a future round
    run skipQuorumSinglePrecommitTest =
        initWith(1, Map("alice" -> 10, "bob" -> 30, "john" -> 60))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == SkipVKOutput(2))

    // Cannot reach Skip via 2f+1 threshold with one prevote and one precommit from the same validator at a future round
    run noSkipQuorumMixedVotesSameValTest =
        initWith(1, Map("alice" -> 60, "bob" -> 30, "john" -> 10))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted != SkipVKOutput(2))

    // Reaching Skip via 2f+1 threshold with one prevote and one precommit from two validators at a future round
    run skipQuorumMixedVotesTwoValsTest =
        initWith(1, Map("alice" -> 20, "bob" -> 50, "john" -> 10))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 1, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Prevote, "john", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "bob", 1, 2, Val("v1")), 1))
        .expect(lastEmitted == SkipVKOutput(2))

    // With weight 6, neither 2 nor 4 clears its respective strict threshold.
    run exactOneThirdFutureRoundTest =
        initWith(1, Map("alice" -> 2, "bob" -> 1, "john" -> 3))
        .then(applyVoteAction(mkVote(Prevote, "alice", 1, 2, Nil), 0))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "bob", 1, 2, Nil), 0))
        .expect(lastEmitted == SkipVKOutput(2))

    run exactTwoThirdsCurrentRoundTest =
        initWith(1, Map("alice" -> 3, "bob" -> 1, "john" -> 2))
        .then(applyVoteAction(mkVote(Precommit, "alice", 1, 0, Val("v1")), 0))
        .then(applyVoteAction(mkVote(Precommit, "bob", 1, 0, Val("v1")), 0))
        .expect(lastEmitted == NoVKOutput)
        .then(applyVoteAction(mkVote(Precommit, "john", 1, 0, Val("v1")), 0))
        .expect(lastEmitted == PrecommitValueVKOutput((0, "v1")))

    run futureCommitQuorumPriorityTest =
        initWith(1, Map("alice" -> 2, "bob" -> 3, "john" -> 2))
        .then(applyVoteAction(mkVote(Precommit, "alice", 1, 1, Val("v1")), 0))
        .then(applyVoteAction(mkVote(Precommit, "bob", 1, 1, Val("v1")), 0))
        .expect(bookkeeper.rounds.get(1).precommits.receivedVotes.size() == 2)

    run repeatedFutureSkipTest =
        initWith(1, Map("alice" -> 2, "bob" -> 3, "john" -> 2))
        .then(applyVoteAction(mkVote(Prevote, "bob", 1, 1, Val("v1")), 0))
        .then(applyVoteAction(mkVote(Precommit, "bob", 1, 1, Val("v1")), 0))
        .expect(bookkeeper.rounds.get(1).precommits.receivedVotes.size() == 1)
}
