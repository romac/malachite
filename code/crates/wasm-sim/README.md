# wasm-sim

A deterministic, in-process simulation of Malachite consensus nodes, with a
browser visualization.

The simulator drives the real `core-consensus` state machine for each node and
plays the roles of the network (configurable delays and drop rate), the timers,
and the application. It runs on a virtual clock with a seeded PRNG, so a given
configuration always produces the same run.

Simplifications:

- Signatures are simulated, and all certificates are accepted as valid.
- Values are proposed in `ProposalOnly` mode; the value proposed by node `i`
  at height `h` is `h * 100 + i`.
- A node that sees a message from a higher height and is still stuck after
  `sync_grace_ms` requests the decided value and commit certificate from that
  peer, a simplified version of ValueSync. Sync messages are never dropped.

## Running the visualization

Requires [`wasm-pack`](https://github.com/wasm-bindgen/wasm-pack).

```sh
cd code/crates/wasm-sim
wasm-pack build --target web --out-dir www/pkg --no-typescript
python3 -m http.server -d www 8000
```

Then open <http://localhost:8000>. Click a node to take it offline or bring it back.

## Tests

```sh
cargo nextest run -p arc-malachitebft-wasm-sim
```
