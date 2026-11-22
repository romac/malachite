# ValueSync Client Quint model

An AI-generated formal [Quint](https://quint-lang.org) model of the Malachite **ValueSync**
client — the mechanism by which a node that has fallen behind catches up on already-decided values
(blocks + commit certificates) from its peers.

The model covers the **client** (syncing) side of the state machine against an abstract **peer
oracle**. It is an executable model: every property below is checked by sampled simulation
(`quint run` / `quint test`), not just typechecked.

## Files

| File | Contents |
|------|----------|
| `valuesync.qnt` | The model: types, pure logic, actions, invariants, observations, concrete instances, scope, and abstractions. |
| `valuesyncTest.qnt` | Deterministic `run` tests for catch-up, retries, and peer-status changes. Imports `valuesync`. |

### Correspondence to the code

The model grounds these files (update the model and re-check its invariants *before* changing them):

- `code/crates/sync/src/handle.rs` — client + server handlers
- `code/crates/sync/src/state.rs` — `State`, peer / range selection
- `code/crates/engine/src/sync.rs` — buffering by `consensus_height`, decide / started-height wiring

## Prerequisites

Install the pinned CLI from the repository root with `make -C code install-quint`. Check it with `quint --version`.

Run all commands from this directory (`specs/synchronization/valuesync/client-model`).

## Typecheck

```bash
quint typecheck valuesync.qnt
quint typecheck valuesyncTest.qnt
```

No output means success.

## Run the deterministic tests

```bash
quint test valuesyncTest.qnt --main=valuesyncTest
```

## Check the invariants (sampled simulation)

The model exposes two concrete instances (modules) to run against:

| Instance | PEERS | BATCH_SIZE | MAX_PARALLEL | TOP |
|----------|-------|-----------|--------------|-----|
| `vs`     | 3     | 2         | 2            | 4   |
| `vsBig`  | 4     | 3         | 3            | 6   |

Quint automatically checks all seven declared invariants and reports how often the ten observations were reached. Run the CI sample locally with:

```bash
quint run valuesync.qnt --main=vs --seed=118 --max-steps=30 --max-samples=1000 --verbosity=2
```

Swap `--main=vs` for `--main=vsBig` to stress a larger configuration.

Expected results:

| Invariant | Meaning | Result |
|-----------|---------|--------|
| `invSyncAboveTip`   | `sync_height > tip` once started | holds |
| `invSyncUncovered`  | `sync_height` never inside a pending range | holds |
| `invDisjoint`       | pending ranges pairwise disjoint | holds |
| `invNoOrphanedOwner`| no pending entry owned by a peer absent from `known` | holds |
| `invNoLostHeight`   | every height in `(tip, sync_height)` is covered by a pending request | holds |
| `invNoCapacityBlockedFrontier` | when no request is outstanding and an eligible peer holds the gap, a frontier request is always possible | holds |
| `invBudgetRespected` | tracked requests awaiting a response never exceed `parallel_requests` | holds |

`invBudgetRespected` bounds the requests ValueSync tracks, not the requests live on the wire. The
engine keeps its own map of transport requests, and two paths drop a tracked entry without ending
its transport request: a height restart clears `pending_requests` while the engine clears only its
value queue, and pruning drops an entry once consensus passes its range. Each orphan ends on its
own response or timeout, so the excess is a window, not a leak. The model has no transport map and
omits the restart path, so it cannot see either.

If an invariant is ever violated (e.g. while iterating on the model), capture the counterexample
trace with `--mbt --out-itf`:

```bash
quint run valuesync.qnt --main=vs --max-steps=30 --max-samples=2000 \
  --invariant=<invariant> --mbt --out-itf=cex.itf.json
```

The failure output includes a seed. Copy it from the `Use --seed=...` line and replay the same
execution with additional detail:

```bash
quint run valuesync.qnt --main=vs --max-steps=30 \
  --invariant=<invariant> --seed=<seed> --backend=rust --verbosity=3
```

## Check reachability

Read the per-observation trace counts under `Observations` in the simulation output. CI requires each of the ten to be reached at least once with its fixed seed. A zero count means the sampled executions missed that state. To save a trace reaching full catch-up to `TOP`, target `wCaughtUp`:

```bash
quint run valuesync.qnt --main=vs --max-steps=30 --max-samples=1000 \
  --observation=wCaughtUp --mbt --out-itf=caught-up.itf.json
```

The search stops at its first match and exits with status 1 if the budget is exhausted.

`wRetriedReservation` reads a latch that the retry actions set, not a shape of the state. Which
entry a retry started from is a property of the step, and every retry ends in an in-flight entry,
so no predicate over the resulting state can tell a reservation retry from an ordinary one.

`wCapacityBlockedFrontier` is the forbidden-state predicate negated by
`invNoCapacityBlockedFrontier`. It should remain unreachable. When checking a suspected regression,
search for a counterexample trace without scripting the interleaving:

```bash
quint run valuesync.qnt --main=vs --max-steps=30 --max-samples=2000 \
  --invariant=invNoCapacityBlockedFrontier --mbt --out-itf=cex.itf.json
```

> Note: `quint run` (sampled) is the primary workflow. For exhaustive bounded checking with
> Apalache, see [Optional: exhaustive verification with Apalache](#optional-exhaustive-verification-with-apalache) below.

## Optional: exhaustive verification with Apalache

The model is Apalache-compatible (height scans use constant `0.to(MAXH)` ranges so Apalache can
encode them), so `quint verify` can bounded-model-check it exhaustively.

Prove an invariant holds up to a depth bound (full BFS — it must rule out every interleaving, so
this is the slow direction):

```bash
quint verify valuesync.qnt --main=vs --invariant=invNoLostHeight --max-steps=10
```

Use `--random-transitions` when hunting a suspected violation; omit it to explore with full BFS.

Apalache writes a `_apalache-out/` directory (gitignored).
