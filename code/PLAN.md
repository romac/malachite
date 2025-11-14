# Facade Pattern Implementation Plan for Consensus Actor

## Overview

Transform the monolithic `Consensus` actor into a facade that delegates to phase-specific handler objects, maintaining a single actor with stable external interface while achieving clean internal separation.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│           Consensus Actor (Facade)                  │
│  - Holds ActorRef (stable external interface)       │
│  - Manages phase transitions                        │
│  - Routes messages to current phase handler         │
└─────────────────────────────────────────────────────┘
                        │
                        ├──────────────┬──────────────┬──────────────┐
                        ▼              ▼              ▼              ▼
            ┌─────────────────┐  ┌─────────────┐  ┌──────────────┐  ┌────────────┐
            │ UnstartedHandler│  │ReadyHandler │  │RecoveringH.. │  │RunningH..  │
            │  - minimal logic│  │ - coord host│  │ - WAL replay │  │ - protocol │
            └─────────────────┘  └─────────────┘  └──────────────┘  └────────────┘
```

---

## 1. Phase Handler Trait Design

### Core Trait Definition

```rust
trait PhaseHandler<Ctx: Context>: Send {
    /// Handle an incoming message
    async fn handle_message(
        &mut self,
        msg: Msg<Ctx>,
        deps: &HandlerDependencies<Ctx>,
        myself: &ActorRef<Msg<Ctx>>,
    ) -> Result<PhaseTransition<Ctx>, ActorProcessingErr>;

    /// Get the current phase (for logging/metrics)
    fn phase(&self) -> Phase;

    /// Get current height (for tracing spans)
    fn height(&self) -> Ctx::Height;

    /// Get current round (for tracing spans)
    fn round(&self) -> Round;

    /// Check if this handler should buffer messages
    fn should_buffer_messages(&self) -> bool;
}

enum PhaseTransition<Ctx: Context> {
    /// Stay in the current phase
    Stay,

    /// Transition to a new phase handler
    TransitionTo(Box<dyn PhaseHandler<Ctx>>),

    /// Transition to a new phase handler and replay buffered messages
    TransitionAndReplay(Box<dyn PhaseHandler<Ctx>>),
}
```

### Shared Dependencies Structure

```rust
struct HandlerDependencies<Ctx: Context> {
    ctx: Ctx,
    params: ConsensusParams<Ctx>,
    consensus_config: ConsensusConfig,
    signing_provider: Arc<dyn SigningProvider<Ctx>>,
    network: NetworkRef<Ctx>,
    host: HostRef<Ctx>,
    wal: WalRef<Ctx>,
    sync: Option<SyncRef<Ctx>>,
    metrics: Metrics,
    tx_event: TxEvent<Ctx>,
}
```

---

## 2. Detailed Handler Implementations

### UnstartedHandler (~50 lines)

**State:**
```rust
pub struct UnstartedHandler<Ctx: Context> {
    msg_buffer: MessageBuffer<Ctx>,
}
```

**Responsibilities:**
- Wait for `NetworkEvent::Listening`
- Buffer all other messages
- Transition to `ReadyHandler` when network is ready

**Key Method:**
```rust
async fn handle_message(...) -> Result<PhaseTransition<Ctx>, ...> {
    match msg {
        Msg::NetworkEvent(NetworkEvent::Listening(address)) => {
            info!(%address, "Network listening, transitioning to Ready");

            // Call host to get initial height
            deps.host.call_and_forward(
                |reply_to| HostMsg::ConsensusReady { reply_to },
                myself,
                // ... forward response
            )?;

            Ok(PhaseTransition::TransitionTo(Box::new(ReadyHandler {
                msg_buffer: self.msg_buffer,
            })))
        }

        msg if should_buffer(&msg) => {
            self.msg_buffer.buffer(msg);
            Ok(PhaseTransition::Stay)
        }

        _ => Ok(PhaseTransition::Stay)
    }
}
```

---

### ReadyHandler (~100 lines)

**State:**
```rust
pub struct ReadyHandler<Ctx: Context> {
    msg_buffer: MessageBuffer<Ctx>,
}
```

**Responsibilities:**
- Receive `StartHeight` or `RestartHeight` message
- Validate validator set
- Create `ConsensusState`
- Check for WAL entries
- Transition to either `RecoveringHandler` (if WAL exists) or `RunningHandler`

**Key Logic Flow:**
```rust
Msg::StartHeight(height, validator_set) => {
    // Validate
    if validator_set.count() == 0 {
        return Err(...);
    }

    // Create consensus state
    let consensus = ConsensusState::new(...);

    // Check WAL
    let wal_entries = deps.wal.fetch(height).await?;

    if !wal_entries.is_empty() {
        // Need to recover
        Ok(PhaseTransition::TransitionTo(Box::new(RecoveringHandler {
            consensus,
            wal_entries,
            msg_buffer: self.msg_buffer,
        })))
    } else {
        // Go straight to running
        Ok(PhaseTransition::TransitionAndReplay(Box::new(RunningHandler {
            consensus,
        })))
    }
}
```

---

### RecoveringHandler (~200-300 lines)

**State:**
```rust
pub struct RecoveringHandler<Ctx: Context> {
    consensus: ConsensusState<Ctx>,
    msg_buffer: MessageBuffer<Ctx>,
    // Recovery-specific state
    recovery_state: RecoveryState,
}

enum RecoveryState {
    NotStarted,
    Replaying {
        entries: Vec<WalEntry<Ctx>>,
        current_index: usize,
    },
    Complete,
}
```

**Responsibilities:**
- Replay WAL entries sequentially
- Buffer incoming messages during replay
- Don't write to WAL during recovery
- Transition to `RunningHandler` when complete

**Key Methods:**
```rust
async fn handle_message(...) -> Result<PhaseTransition<Ctx>, ...> {
    match &mut self.recovery_state {
        RecoveryState::NotStarted => {
            // Start replaying
            self.start_replay(deps).await?;
            Ok(PhaseTransition::Stay)
        }

        RecoveryState::Replaying { entries, current_index } => {
            // Continue replaying one entry at a time
            if *current_index < entries.len() {
                self.replay_entry(&entries[*current_index], deps).await?;
                *current_index += 1;
                Ok(PhaseTransition::Stay)
            } else {
                // Recovery complete
                self.recovery_state = RecoveryState::Complete;
                self.complete_recovery(deps).await
            }
        }

        RecoveryState::Complete => {
            // Transition to running
            Ok(PhaseTransition::TransitionAndReplay(Box::new(RunningHandler {
                consensus: self.consensus,
            })))
        }
    }
}

async fn replay_entry(&mut self, entry: &WalEntry<Ctx>, deps: &HandlerDependencies<Ctx>) {
    // Process vote, proposal, timeout, etc.
    // Call process_input() but with phase = Recovering (skips WAL writes)
}
```

**Alternative Design (Simpler):**
Instead of state machine, just replay all entries in `handle_message` on first call:
```rust
async fn handle_message(...) -> Result<PhaseTransition<Ctx>, ...> {
    if !self.replayed {
        // Replay all WAL entries synchronously
        for entry in &self.wal_entries {
            self.replay_entry(entry, deps).await?;
        }
        self.replayed = true;

        // Immediately transition to running
        return Ok(PhaseTransition::TransitionAndReplay(
            Box::new(RunningHandler { consensus: self.consensus })
        ));
    }

    // Buffer any messages that arrive during replay
    if should_buffer(&msg) {
        self.msg_buffer.buffer(msg);
    }

    Ok(PhaseTransition::Stay)
}
```

---

### RunningHandler (~800-1000 lines - most of current logic)

**State:**
```rust
pub struct RunningHandler<Ctx: Context> {
    consensus: ConsensusState<Ctx>,
}
```

**Responsibilities:**
- Handle all consensus protocol messages (votes, proposals, certificates)
- Process timeouts
- Handle network events (peer connections, sync responses)
- Coordinate with host for values
- Write to WAL
- Potentially transition back to `RecoveringHandler` on `RestartHeight`

**Key Message Categories:**
```rust
async fn handle_message(...) -> Result<PhaseTransition<Ctx>, ...> {
    match msg {
        // Consensus protocol messages
        Msg::NetworkEvent(NetworkEvent::Vote(from, vote)) => {
            self.handle_vote(vote, deps, myself).await?;
            Ok(PhaseTransition::Stay)
        }

        Msg::NetworkEvent(NetworkEvent::Proposal(from, proposal)) => {
            self.handle_proposal(proposal, deps, myself).await?;
            Ok(PhaseTransition::Stay)
        }

        Msg::TimeoutElapsed(timeout) => {
            self.handle_timeout(timeout, deps, myself).await?;
            Ok(PhaseTransition::Stay)
        }

        Msg::ProposeValue(value) => {
            self.handle_propose_value(value, deps, myself).await?;
            Ok(PhaseTransition::Stay)
        }

        Msg::ReceivedProposedValue(value, origin) => {
            self.handle_received_value(value, origin, deps, myself).await?;
            Ok(PhaseTransition::Stay)
        }

        // Lifecycle messages
        Msg::StartHeight(height, validator_set) => {
            // Normal height progression
            self.transition_to_next_height(height, validator_set, deps).await
        }

        Msg::RestartHeight(height, validator_set) => {
            // Error recovery - need to reset WAL and restart
            deps.wal.reset(height).await?;

            let consensus = ConsensusState::new(...);

            Ok(PhaseTransition::TransitionTo(Box::new(RunningHandler {
                consensus,
            })))
        }

        Msg::DumpState(reply_to) => {
            let dump = StateDump::new(&self.consensus);
            reply_to.send(Some(dump))?;
            Ok(PhaseTransition::Stay)
        }

        _ => Ok(PhaseTransition::Stay)
    }
}
```

**Helper Methods (private):**
```rust
impl<Ctx: Context> RunningHandler<Ctx> {
    async fn handle_vote(&mut self, vote: Vote<Ctx>, deps: &HandlerDependencies<Ctx>, ...) {
        // Process through core consensus
        process_input!(
            input: ConsensusInput::Vote(vote),
            state: &mut self.consensus,
            ...
        )
    }

    async fn handle_proposal(...) { ... }
    async fn handle_timeout(...) { ... }
    async fn process_sync_response(...) { ... }
    // etc.
}
```

---

## 3. Refactored Consensus Actor

### Simplified Actor Structure

```rust
pub struct Consensus<Ctx: Context> {
    dependencies: Arc<HandlerDependencies<Ctx>>,
    span: tracing::Span,
}

pub struct State<Ctx: Context> {
    shared: SharedState,
    handler: Box<dyn PhaseHandler<Ctx>>,
}
```

### Simplified Message Handling

```rust
async fn handle(
    &self,
    myself: ActorRef<Msg<Ctx>>,
    msg: Msg<Ctx>,
    state: &mut State<Ctx>,
) -> Result<(), ActorProcessingErr> {
    // Check if we should buffer (for Unstarted/Ready/Recovering)
    if state.handler.should_buffer_messages() && should_buffer(&msg) {
        // Handler will buffer it
    }

    // Delegate to handler
    let transition = state.handler
        .handle_message(msg, &self.dependencies, &myself)
        .await?;

    // Handle transition
    match transition {
        PhaseTransition::Stay => {
            // Nothing to do
        }

        PhaseTransition::TransitionTo(new_handler) => {
            let old_phase = state.handler.phase();
            let new_phase = new_handler.phase();
            info!(?old_phase, ?new_phase, "Phase transition");

            state.handler = new_handler;
        }

        PhaseTransition::TransitionAndReplay(new_handler) => {
            let old_phase = state.handler.phase();
            let new_phase = new_handler.phase();
            info!(?old_phase, ?new_phase, "Phase transition with replay");

            // Extract buffered messages from old handler
            let buffered = state.handler.take_buffer(); // new method
            state.handler = new_handler;

            // Replay buffered messages
            for msg in buffered {
                self.handle(myself.clone(), msg, state).await?;
            }
        }
    }

    Ok(())
}
```

---

## 4. Shared State Management

### SharedState (unchanged)
```rust
struct SharedState {
    timers: Timers,
    timeouts: Timeouts,
    connected_peers: BTreeSet<PeerId>,
}
```

Handlers access this through the main actor or pass references as needed.

---

## 5. Effect Handling Strategy

**Option A: Keep in Main Actor (Recommended)**
- `process_input()` stays in `Consensus` actor
- Handlers call back to actor for effect processing
- Pass `&Consensus` reference or effect handler to handlers

**Option B: Move to Handlers**
- Each handler implements its own effect handling
- More duplication but more isolated
- RunningHandler would have most of the effect logic

**Recommended: Option A** - Keep effect handling centralized since it's mostly the same across phases (except WAL writes during recovery).

---

## 6. Testing Strategy

### Unit Tests for Each Handler

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn unstarted_buffers_messages() {
        let mut handler = UnstartedHandler::new();
        let msg = Msg::NetworkEvent(NetworkEvent::Vote(...));

        let result = handler.handle_message(msg, &deps, &actor_ref).await;

        assert!(matches!(result, Ok(PhaseTransition::Stay)));
        assert_eq!(handler.msg_buffer.len(), 1);
    }

    #[tokio::test]
    async fn ready_transitions_to_running_without_wal() {
        let mut handler = ReadyHandler::new();
        let msg = Msg::StartHeight(height, validator_set);

        // Mock WAL to return empty entries
        let result = handler.handle_message(msg, &deps, &actor_ref).await;

        assert!(matches!(result, Ok(PhaseTransition::TransitionAndReplay(_))));
    }

    #[tokio::test]
    async fn running_processes_vote() {
        let mut handler = RunningHandler::new(...);
        let vote = create_test_vote();
        let msg = Msg::NetworkEvent(NetworkEvent::Vote(peer, vote));

        let result = handler.handle_message(msg, &deps, &actor_ref).await;

        assert!(matches!(result, Ok(PhaseTransition::Stay)));
        // Assert vote was processed in consensus state
    }
}
```

### Integration Tests
- Test full actor lifecycle through all phases
- Test phase transitions with actual messages
- Test WAL recovery flow

---

## 7. Migration Path

### Step 1: Create Infrastructure (Non-Breaking)
1. Create `consensus/handlers.rs` with trait
2. Create `consensus/shared.rs` with `HandlerDependencies`
3. Keep existing code working

### Step 2: Implement Handlers One-by-One
1. Start with `UnstartedHandler` (simplest)
2. Then `ReadyHandler`
3. Then `RecoveringHandler`
4. Finally `RunningHandler` (largest)
5. Each handler gets unit tests as implemented

### Step 3: Switch Actor Implementation
1. Change `State` to use `Box<dyn PhaseHandler>`
2. Update `handle()` method to delegate
3. Run full test suite
4. Fix any issues

### Step 4: Cleanup
1. Remove old `ConsensusPhase` enum
2. Remove old phase-specific methods from main actor
3. Clean up unused code

---

## 8. File Organization

```
crates/engine/src/consensus/
├── mod.rs                  (main actor, ~300 lines)
├── handlers.rs             (trait definition, ~100 lines)
├── handlers/
│   ├── mod.rs
│   ├── unstarted.rs       (~50 lines)
│   ├── ready.rs           (~100 lines)
│   ├── recovering.rs      (~250 lines)
│   └── running.rs         (~900 lines)
├── shared.rs              (SharedState, Dependencies, ~100 lines)
└── state_dump.rs          (existing, unchanged)

Total: ~1800 lines (vs current 1618, slight increase but much better organized)
```

---

## 9. Key Design Decisions

### Why Trait Objects Instead of Enums?
- **Pros**: Clean separation, easier to test, no giant match statements
- **Cons**: Slight runtime overhead (vtable dispatch), heap allocation
- **Decision**: Use trait objects - the benefits outweigh minimal overhead

### Why Not Separate Actor Per Phase?
- **Avoided**: Actor lifecycle complexity, message loss, ref updates
- **Keeping**: Single actor with single ActorRef - simpler coordination

### Where Does `process_input()` Live?
- **Decision**: Keep in main `Consensus` actor
- **Reasoning**: Reused across handlers, centralizes effect handling

### How to Handle SharedState?
- **Decision**: Keep in actor, pass mutable references to handlers as needed
- **Alternative**: Could move into `HandlerDependencies` but needs `RefCell`

---

## 10. Benefits Summary

✅ **Clean Separation**: Each handler is focused and testable
✅ **Single ActorRef**: No coordination complexity
✅ **Type Safety**: Each handler has only methods it needs
✅ **Easy Testing**: Mock dependencies, test handlers independently
✅ **Better Organization**: Logic grouped by phase, not scattered
✅ **No Performance Cost**: Just function calls, no actor overhead
✅ **Gradual Migration**: Can implement handlers one at a time
✅ **Future Flexibility**: Easy to add new phases or modify existing ones

---

## Implementation Checklist

- [ ] Step 1: Create infrastructure
  - [ ] Create `consensus/handlers.rs` with trait definition
  - [ ] Create `consensus/shared.rs` with `HandlerDependencies`
  - [ ] Add `PhaseTransition` enum

- [ ] Step 2: Implement handlers
  - [ ] `UnstartedHandler` with unit tests
  - [ ] `ReadyHandler` with unit tests
  - [ ] `RecoveringHandler` with unit tests
  - [ ] `RunningHandler` with unit tests

- [ ] Step 3: Refactor main actor
  - [ ] Update `State` to use `Box<dyn PhaseHandler>`
  - [ ] Update `handle()` to delegate to handlers
  - [ ] Update `pre_start()` to create initial handler
  - [ ] Handle phase transitions

- [ ] Step 4: Testing
  - [ ] Run existing test suite
  - [ ] Add integration tests for phase transitions
  - [ ] Test WAL recovery flow

- [ ] Step 5: Cleanup
  - [ ] Remove old `ConsensusPhase` enum
  - [ ] Remove old phase-specific methods
  - [ ] Update documentation
  - [ ] Code review

---

## Estimated Effort

- **Implementation**: 4-6 hours
- **Testing**: 2-3 hours
- **Review & Cleanup**: 1-2 hours
- **Total**: ~8-11 hours

## Risk Assessment

- **Low Risk**: Internal refactoring only, no external API changes
- **Easy Rollback**: Can keep old code in place during migration
- **High Value**: Significant maintainability improvement
