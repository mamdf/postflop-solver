# postflop-solver — agent guide

CFR poker solver. Its tests **run the solver**: a raw `cargo test` can hang or pin
every core.

## Tests

- As an agent, run `just test-agent` or `just test-one <name>`, never raw `cargo test`.
  They cap cores and kill a hung run (`justfile`). The cap is deliberately not
  global: server, manual runs and benches stay at full power.
- Heavy `#[ignore]`d presets: `just test-heavy`, only on explicit request.
- New tests: kuhn/leduc-scale spots, few iterations. If it can't be cheap, `#[ignore]` it.
- Benches (`just bench`): long, full CPU, uncapped; run deliberately in the background.

## Node locking (strategy locking) — it EXISTS

Not the concurrency locks of `mutex_like.rs`. API: `src/game/interpreter.rs`,
`src/game/cursor.rs`; example `examples/node_locking.rs`; over HTTP: the
`postflop-solver-api` README.

- With locked nodes, `compute_exploitability` is relative to the restricted game.
- Isomorphic turn/river cards alias one node: lock via the representative cards of
  `available_actions()`, or the last write wins.
- Evaluating another solver's strategy: rustdoc of `src/profile.rs`. Dumps are GBs:
  run deliberately, full power, in the background.

## Cross-repo consumers

Before changing the exposed surface, run from the workspace root:
`uv run tooling/workspace-index/impact.py postflop-solver`
