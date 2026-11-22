module driverTest {
  import types.* from "../../types"
  import consensus.* from "../../consensus"
  import driver.* from "../../driver"

  pure val weights = Map("v1" -> 2, "v2" -> 3, "v3" -> 2)
  pure val height = 1
  pure def leader(r) = proposer(weights, height, r)

  var model: DriverState
  var event: DriverInput
  var sequence: int
  var effects: List[ConsensusOutput]
  // The application supplies the same proposer schedule to both drivers.
  var leaders: List[Address]

  action initAt(roleRound) = all {
    model' = initDriver(leader(roleRound), weights, height, []),
    event' = NoDInput, sequence' = 0, effects' = [],
    leaders' = [leader(0), leader(1), leader(2), leader(3)],
  }

  type Turn = { ds: DriverState, outputs: List[ConsensusOutput] }

  pure def applyInput(turn: Turn, input: DriverInput): Turn = {
    val result = driver(turn.ds, input)
    // An inapplicable round-machine rule has no outward driver effect.
    val outputs = match result.out {
      | NoConsensusOutput => turn.outputs
      | ErrorOutput(_) => turn.outputs
      | _ => turn.outputs.append(result.out)
    }
    { ds: result.ds, outputs: outputs }
  }

  pure def drainOne(turn: Turn): Turn =
    if (turn.ds.pendingStepChange != NoStep) applyInput(turn, StepChangeDInput)
    else if (turn.ds.pendingInputs != Set()) applyInput(turn, PendingDInput(NoConsensusInput))
    else turn

  action deliver(input: DriverInput): bool = {
    val first = applyInput({ ds: model, outputs: [] }, input)
    val turn = range(0, 12).foldl(first, (acc, _) => drainOne(acc))
    all {
      // Fail if a fixture ever exhausts the drain budget with pending work.
      assert(turn.ds.pendingStepChange == NoStep and turn.ds.pendingInputs == Set()),
      model' = turn.ds, effects' = turn.outputs, event' = input,
      sequence' = sequence + 1, leaders' = leaders,
    }
  }

  action start(roleRound) = initAt(roleRound).then(deliver(StartDInput))
  action proposal(r, value, validRound) =
    deliver(ProposalDInput(mkProposal(leader(r), height, r, value, validRound)))
  action vote(kind, who, r, value) =
    deliver(VoteDInput(mkVote(kind, who, height, r, value)))
  action certificate(kind, r, value) =
    deliver(CertificateDInput(Set("v1", "v2").map(who =>
      mkVote(kind, who, height, r, Val(value)))))

  run proposerGetsValueTest = start(0)
    .then(deliver(ProposeValueDInput("block")))
    .then(proposal(0, "block", -1))
    .expect(model.cs.step == PrevoteStep)

  run commitByVotesTest = start(1)
    .then(proposal(0, "block", -1))
    .then(vote(Precommit, "v1", 0, Val("block")))
    .then(vote(Precommit, "v2", 0, Val("block")))
    .expect(model.chain.length() == 1)

  run commitBeforeProposalTest = start(1)
    .then(vote(Precommit, "v1", 0, Val("block")))
    .then(vote(Precommit, "v2", 0, Val("block")))
    .then(proposal(0, "block", -1))
    .expect(model.cs.step == CommitStep)

  run commitCertificateTest = start(1)
    .then(proposal(0, "block", -1))
    .then(certificate(Precommit, 0, "block"))
    .expect(model.cs.step == CommitStep)

  run duplicateVoteTest = start(1)
    .then(deliver(TimeoutDInput(ProposeTimeout)))
    .then(vote(Prevote, "v1", 0, Nil))
    .then(vote(Prevote, "v1", 0, Nil))
    .then(vote(Prevote, "v2", 0, Nil))
    .expect(model.cs.step == PrecommitStep)

  run invalidProposalTest = start(1)
    .then(proposal(0, "invalid", -1))
    .expect(model.cs.step == PrevoteStep)

  run timeoutsAndNewRoundTest = start(2)
    .then(deliver(TimeoutDInput(ProposeTimeout)))
    .then(vote(Prevote, "v1", 0, Val("block")))
    .then(vote(Prevote, "v2", 0, Nil))
    .then(deliver(TimeoutDInput(PrevoteTimeout)))
    .then(vote(Precommit, "v1", 0, Nil))
    .then(vote(Precommit, "v2", 0, Nil))
    .then(deliver(TimeoutDInput(PrecommitTimeout)))
    .expect(model.cs.round == 1 and model.cs.step == ProposeStep)

  run adjacentProposerTest = start(1)
    .then(vote(Prevote, "v2", 1, Nil))
    .expect(model.cs.round == 1)

  run decidePastProposalTest = start(2)
    .then(proposal(0, "block", -1))
    .then(vote(Prevote, "v2", 1, Nil))
    .then(vote(Precommit, "v1", 0, Val("block")))
    .then(vote(Precommit, "v2", 0, Val("block")))
    .expect(model.cs.round == 1 and model.chain[0].decision.round == 0)

  run invalidProposalWithPriorRoundTest = start(2)
    .then(vote(Prevote, "v2", 1, Nil))
    .then(proposal(1, "invalid", 0))
    .expect(sequence == 3)

  run currentPolkaTest = start(1)
    .then(proposal(0, "block", -1))
    .then(vote(Prevote, "v1", 0, Val("block")))
    .then(vote(Prevote, "v2", 0, Val("block")))
    .expect(model.cs.lockedValue == Val("block"))

  run cachedNilPolkaTest = start(1)
    .then(vote(Prevote, "v1", 0, Nil))
    .then(vote(Prevote, "v2", 0, Nil))
    .then(proposal(0, "block", -1))
    .expect(sequence == 4)

  run futurePolkaTest = start(1)
    .then(certificate(Prevote, 2, "block"))
    .expect(sequence == 2)

  run nonAdjacentProposerTest = start(2)
    .then(vote(Prevote, "v2", 2, Nil))
    .expect(sequence == 2)

  run futureCommitWithProposalTest = start(2)
    .then(proposal(1, "block", -1))
    .then(vote(Precommit, "v1", 1, Val("block")))
    .then(vote(Precommit, "v2", 1, Val("block")))
    .expect(sequence == 4)
}
