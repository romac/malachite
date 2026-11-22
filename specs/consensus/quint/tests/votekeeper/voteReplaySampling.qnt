module voteReplaySampling {
  import types.* from "../../types"
  import votekeeper.* from "../../votekeeper"
  import voteKeeperProperties.* from "./votekeeperModels"

  pure val weights = Map("alice" -> 1, "bob" -> 2, "john" -> 3)
  var sequence: int
  var observedRound: Round

  action init = all { initWith(1, weights), sequence' = 0, observedRound' = 0 }

  // One vote per (round, type, signer), with repeat delivery and interleaved rounds.
  // A future vote from john crosses one third. The caller adopts that round
  // before the next delivery. Two deadlines guarantee every trace exercises a skip.
  action step = {
    nondet advance = Set(false, true).oneOf()
    val future = observedRound < 2 and (advance or sequence == 6 or sequence == 14)
    nondet r = (if (future) Set(observedRound + 1) else 0.to(observedRound)).oneOf()
    nondet signer = (if (future) Set("john") else weights.keys()).oneOf()
    nondet kind = Set(Prevote, Precommit).oneOf()
    val value = if (r == 0) Val("v1")
      else if (r == 1 and signer == "john") Val("v2")
      else Nil
    all {
      applyVoteAction(mkVote(kind, signer, 1, r, value), observedRound),
      sequence' = sequence + 1,
      observedRound' = if (future) r else observedRound,
    }
  }
}
