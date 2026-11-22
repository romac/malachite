# Quint-to-Rust replay

The suite generates ITF traces from the repository's Quint models and replays their external events against fresh Rust components. Install the repository's pinned Quint CLI and run the suite from the Rust workspace. From the repository root:

```sh
make -C code install-quint
cd code
QUINT_SEED=118 cargo test --locked -p arc-malachitebft-test-mbt --all-features
```

| Rust boundary | What the replay observes |
| --- | --- |
| Round state machine | Accepted and rejected transitions, ordered round actions, lock and valid value, and the local and certified rounds of a decision. |
| Vote keeper | Fourteen threshold and vote-order cases, plus 32 seeded schedules of 24 deliveries. Each schedule includes a future-round skip. Four additional traces must end at a value precommit quorum, an any-value polka, a nil polka and a round skip, respectively. Each delivery checks the returned result, stored outputs, weights and complete stored votes. |
| Consensus driver | Fifteen scenarios deliver real external inputs. Each compares the ordered effects and the full round, step, lock, valid value and decision at the end of the input. |
| Four-driver network | A complete honest round delivers only messages previously emitted by one of the Rust drivers. The final check compares each replica and both three-vote decision certificates, then evaluates Quint's agreement invariant. |
| Core consensus and host | A signed, valid proposal arrives while the host verdict is pending. The parts-based mode also exercises both arrival orders. The test checks actual signing and publication against the test fixture's host-acceptance rule. |
| Value sync client | Parallel ranges, out-of-order and partial responses, peer disconnect, and retry after a decision. The replay compares request IDs, peers, ranges, reservations, cursors and values forwarded to consensus. |
| Proposal-part reassembly | All 24 permutations of four messages must produce the exact ordered batch. Each completed stream also has a Rust-side check that the shared map releases its stream ID. |

The default run checks every observed difference against its recorded case, event and values. A new difference fails the test. Removing an existing difference also fails until its expectation is removed alongside the correction. The host timing and stream-ID lifetime expectations are explicit test contracts at boundaries that the original Quint models abstract.

The default evaluator is `rust`. Set `QUINT_BACKEND=typescript` to run the same checks with the TypeScript evaluator. CI runs both with one seed. The vote keeper uses `--observation` to save a trace for each required outcome, checks the last ITF state and compares every delivery with the Rust vote keeper. The driver compares concrete addresses and labels them by proposer round in its difference report.

Use `MBT_STRICT=1` to require agreement at every checkpoint. Set `KEEP_TEMP=1` and pass `-- --nocapture` to print the directories of generated traces. Reuse the reported `QUINT_SEED` to replay a run.

The sync client replay ends at the effect that hands values to consensus. Network transport, the engine actor's out-of-order buffer, WAL recovery, and peer discovery have separate boundaries.
