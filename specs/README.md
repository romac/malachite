# Malachite Specifications

This directory holds specifications of various Malachite components or protocols.
It also covers specifications related to external dependencies, i.e., application-level concerns.
For example, Malachite comprises a library implementing the [Tendermint consensus algorithm][tendermint-arxiv] in Rust, which is specified below.
The specifications also cover networking and synchronization protocols.

- [Consensus algorithm and implementation](./consensus/README.md)
- [Network design and requirements](./network/README.md)
- [Synchronization protocols](./synchronization/README.md)
- [Block streaming protocol](./block-streaming/README.md)

## Running Quint locally

From the repository root, install the CLI version pinned in `specs/quint-version` and run the same standalone checks as CI:

```sh
make -C code install-quint
find specs -name '*.qnt' | ./scripts/quint-forall.sh typecheck
./scripts/quint-tests.py --backend rust
./scripts/quint-tests.py --backend typescript
```

The standalone suites use seed 118 and up to 100 samples per run. TypeScript must pass every run. For the Rust evaluator, [the baseline](quint-rust-known-failures.tsv) records 20 existing failures by suite, run, diagnostic and source location. A change in any failure or an unexpected pass fails CI. Use `--strict` to require every run to pass. The [model-based tests](../code/crates/test/mbt/README.md) exercise the Rust implementation with traces from both evaluators.

[tendermint-arxiv]: https://arxiv.org/abs/1807.04938
