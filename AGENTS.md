# postflop-solver — agent guide

This crate is a CFR poker solver. Its tests **run the solver**, so a careless
`cargo test` can either hang for minutes or pin every CPU core. Read the rules
below **before** running any `cargo test` / `cargo bench` in this repo.

## Running tests — MANDATORY rules for automated agents

**1. As an automated agent, run the suite via the capped recipe — never raw `cargo test`:**

```sh
just test-agent          # = RAYON_NUM_THREADS=4 RUST_TEST_THREADS=2 cargo test --features zstd
just test-one <name>     # single test under the same cap, e.g. just test-one kuhn
```

If `just` is unavailable, use the raw equivalent:

```sh
RAYON_NUM_THREADS=4 RUST_TEST_THREADS=2 cargo test --features zstd
```

Why the cap: Rayon defaults to **all 12 logical cores** and the test harness
runs many tests in parallel → oversubscription → ~95% CPU and a frozen machine.
The cap (4 Rayon cores, 2 parallel tests) keeps the box responsive. The cap is
**deliberately not global** — the server, manual runs, and calibration benches
must stay at full power, so it lives only in the `*-agent` recipes, not in
`.cargo/config.toml`.

**2. Always bound the run — never block on it blind:**

```sh
timeout 300 just test-agent        # abort instead of waiting forever
```

Or run it in the background and poll. If a run exceeds its budget below by ~3×,
**stop and investigate** — do not keep waiting.

**3. Tests are already fast — they no longer need `--release`.** `Cargo.toml`
sets `[profile.test] opt-level = 3`, so `cargo test` runs at release speed
**while keeping `debug-assertions` + overflow checks ON** (stricter than
`--release`). Adding `--release` only disables those checks; don't bother.

## Time budget (this host: Ryzen 5 5600X, 6c/12t, capped to 4 Rayon cores)

Numbers are wall time for a **warm build** (first build adds opt-level-3 compile
time, ~1–2 min, one-off). If a run takes ≳3× its budget, something is wrong.

<!-- BUDGET:LIB -->
| Command | Budget (warm) | Abort if > | Notes |
|---|---|---|---|
| `just test-agent` (full suite) | ~30 s | 90 s | all non-ignored tests; ~314% CPU (cap confirmed) |
| `just test-one kuhn` | <1 s | 5 s | tiny tree, 10k iters |
| `just test-one leduc` | <1 s | 5 s | small tree, 10k iters |
| `cargo test … --test compact_solution` | ~8 s | 25 s | solves web case ×3 + disk I/O |
| `cargo test … --test web_case_regression` | ~3 s | 10 s | solves web case + CSV navigation |
<!-- /BUDGET:LIB -->

The heavy `#[ignore]`d presets (`solve_pio_preset_*`, `test_bunching_wizard`,
`test_bunching_independent_4`) take **minutes and several GB of RAM**. Run them
only on explicit request, full-power and deliberate:

```sh
just test-heavy                    # cargo test --features zstd -- --ignored
```

## Adding a new test

- Keep it cheap: tiny spots (kuhn/leduc scale) and **low `max_iterations`**.
  Most correctness tests need only 1–10 iterations; reserve big spots / high
  iteration counts for `#[ignore]`.
- Annotate the expected cost so the next agent knows what "normal" is:
  ```rust
  // budget: ~2s @ RAYON_NUM_THREADS=4 (warm)
  ```
- If you can't keep it cheap, gate it behind `#[ignore]` and document why.
- After adding it, measure once with `just test-one <name>` and record the
  number in the comment and (if it's a standing heavy test) in the table above.

## Benchmarks

`cargo bench` (`just bench`) uses the `bench` profile, which is **already
release-optimized** — the issue is duration + full CPU, not opt-level. Benches
are long; run them deliberately with a timeout/background, and do **not** cap
their cores (a capped bench is not representative).

<!-- BEGIN AUTOGEN:consumed-by (workspace.contracts.yaml — do not edit by hand) -->
## Consumed by

If you change this repo's exposed surface, these repos may break — ⚠ marks an
edge that declares a risk. Full `what`/`risk` prose:
`uv run tooling/workspace-index/impact.py libs/rust/postflop-solver`.
Refresh: `uv run tooling/workspace-index/gen_consumed_by.py --write`.

- **`adapters/rust/postflop-solver-api`** (cargo-path) — CFR engine… ⚠
<!-- END AUTOGEN:consumed-by -->
