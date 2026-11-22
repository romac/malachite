module agreementReplayTest {
  import TendermintDSL(
    validators = Set("v1", "v2", "v3", "v4"),
    validatorSet = Map("v1" -> 1, "v2" -> 1, "v3" -> 1, "v4" -> 1),
    Faulty = Set(), Values = Set("block", "nextBlock"),
    Rounds = Set(0, 1, 2, 3), Heights = Set(0)
  ).* from "../../TendermintDSL"

  pure val weights = Map("v1" -> 1, "v2" -> 1, "v3" -> 1, "v4" -> 1)

  // Report the actual invariant alongside the model's final per-replica decisions.
  var agreement: bool
  var roundZeroProposer: str

  action observed(a) = all { a, agreement' = AgreementInv, roundZeroProposer' = proposer(weights, 0, 0) }

  run honestQuorumsTest = all { init, agreement' = true, roundZeroProposer' = proposer(weights, 0, 0) }
    .then(observed(everyoneReceivesProposal(
      correctList, validatorList, weights, 0, 0, "block")))
    .then(observed(fromPrevoteToPrecommit(
      ["v1", "v2", "v3"], correctList, validatorList, weights, 0, 0, Val("block"))))
    .then(observed(ListDeliverAllVotes(
      Precommit, ["v1", "v2", "v3"], ["v1"], weights, 0, 0, Val("block"))))
    .then(observed(ListDeliverAllVotes(
      Precommit, ["v2", "v3", "v4"], ["v2"], weights, 0, 0, Val("block"))))
    .then(observed(3.reps(_ => ListTakeAStep(["v1", "v2"]))))
    .expect(system.get("v1").es.chain[0].decision == system.get("v2").es.chain[0].decision)
    .expect(system.get("v1").es.chain[0].commit != system.get("v2").es.chain[0].commit)
    .then(all { unchangedAll, agreement' = AgreementInv, roundZeroProposer' = proposer(weights, 0, 0) })
    .expect(system.get("v1").es.chain.length() == 1)
}
