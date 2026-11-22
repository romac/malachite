# Quint-to-Rust replay

The suite generates ITF traces from the repository's Quint models and replays their external events against fresh Rust components. Run it from `code/` with Quint 0.27.0 on `PATH`, as used by the MBT workflow.

```sh
QUINT_SEED=118 cargo test --locked -p arc-malachitebft-test-mbt --all-features
```

| Rust boundary | What the replay observes |
| --- | --- |
| Round state machine | Accepted and rejected transitions, ordered round actions, lock and valid value, and the local and certified rounds of a decision. |
| Vote keeper | Fourteen threshold and vote-order cases, plus 32 seeded schedules of 24 deliveries. Each schedule includes a future-round skip. Each delivery checks the returned result, stored outputs, weights and complete stored votes. |
| Consensus driver | Fifteen scenarios deliver real external inputs. Each compares the ordered effects and the full round, step, lock, valid value and decision at the end of the input. |
| Four-driver network | A complete honest round delivers only messages previously emitted by one of the Rust drivers. The final check compares each replica and both three-vote decision certificates, then evaluates Quint's agreement invariant. |
| Core consensus and host | A signed, valid proposal arrives while the host verdict is pending. The parts-based mode also exercises both arrival orders. The test checks actual signing and publication against the test fixture's host-acceptance rule. |
| Value sync client | Parallel ranges, out-of-order and partial responses, peer disconnect, and retry after a decision. The replay compares request IDs, peers, ranges, reservations, cursors and values forwarded to consensus. |
| Proposal-part reassembly | All 24 permutations of four messages must produce the exact ordered batch. Each completed stream also has a Rust-side check that the shared map releases its stream ID. |

The default run checks every observed difference against its recorded case, event and values. A new difference fails the test. Removing an existing difference also fails until its expectation is removed alongside the correction. The host timing and stream-ID lifetime expectations are explicit test contracts at boundaries that the original Quint models abstract.

With Quint 0.33.1, set `QUINT_BACKEND=typescript` or `QUINT_BACKEND=rust` to select the evaluator for both named and generated traces. The driver compares concrete addresses and labels them by proposer round in its difference report.

Use `MBT_STRICT=1` to require agreement at every checkpoint. Set `KEEP_TEMP=1` and pass `-- --nocapture` to print the directories of generated traces. Reuse the reported `QUINT_SEED` to replay a run.

The sync client replay ends at the effect that hands values to consensus. Network transport, the engine actor's out-of-order buffer, WAL recovery, and peer discovery have separate boundaries.
