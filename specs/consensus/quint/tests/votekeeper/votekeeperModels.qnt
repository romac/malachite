// -*- mode: Bluespec; -*-

// These properties read the standalone vote keeper initialized by initWith.
// The driver instead stores a separate bookkeeper inside each node.
module voteKeeperProperties {
    import types.* from "../../types"
    import votekeeper.* from "../../votekeeper"

    invariant validVoteWeights = Inv

    observation emitPrecommitValue = match lastEmitted {
        | PrecommitValueVKOutput(_) => true | _ => false
    }
    observation emitPolkaAny = match lastEmitted {
        | PolkaAnyVKOutput(_) => true | _ => false
    }
    observation emitPolkaNil = match lastEmitted {
        | PolkaNilVKOutput(_) => true | _ => false
    }
    observation emitSkip = match lastEmitted {
        | SkipVKOutput(_) => true | _ => false
    }
}

// ****************************************************************************
// Vote Bookkeeper State Machine
// **************************************************************************** 

module votekeeperSM  {

    import types.* from "../../types"
    import votekeeper.* from "../../votekeeper"
    export votekeeper.*
    import voteKeeperProperties.*
    export voteKeeperProperties.*

    // ************************************************************************
    // Model parameters 
    // ************************************************************************

    const INITIAL_HEIGHT: Height
    const INITIAL_TOTAL_WEIGHT: Weight
    const ADDRESS_WEIGHTS: Address -> Weight // an address has a constant weight during a height
    const ROUNDS: Set[Round]
    const VALUES: Set[NonNilValue]

    // ************************************************************************
    // State machine
    // ************************************************************************

    action init = 
        initWith(INITIAL_HEIGHT, ADDRESS_WEIGHTS)
    
    action step = 
        nondet voteType = oneOf(Set(Prevote, Precommit))
        nondet round = oneOf(ROUNDS)
        nondet value = oneOf(VALUES.map(v => Val(v)).union(Set(Nil)))
        nondet address = oneOf(ADDRESS_WEIGHTS.keys())
        val height = INITIAL_HEIGHT
        val vote: Vote = mkVote(voteType, address, height, round, id(value))
        applyVoteAction(vote, round)
}

// ****************************************************************************
// Vote Bookkeeper Models
// **************************************************************************** 

module votekeeperModels {
    
    import votekeeperSM(
        INITIAL_HEIGHT = 1,
        INITIAL_TOTAL_WEIGHT = 100,
        ADDRESS_WEIGHTS = Map("alice" -> 10, "bob" -> 30, "john" -> 60),
        ROUNDS = 0.to(2),
        VALUES = Set("v1", "v2")
    ) as VK1

}
