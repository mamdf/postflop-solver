# Compact solution save

Save a solved game to a small file and reload it into a **fully navigable** solution —
strategy, EV, and equity queryable at every node — without re-solving. A River-mode save
stores only the average strategy on disk and recomputes the counterfactual values when
the file is loaded.

This is packaging, not a new format: [`save_solution`]/[`load_solution`] wrap the existing
[`save_data_to_file`]/[`load_data_from_file`] primitives and produce byte-identical files.

## Quick path

```rust
use postflop_solver::*;

// ... build `game`, `allocate_memory(false)`, then `solve(...)` ...

// Smallest fully navigable artifact (River mode + zstd). Requires the `zstd` feature.
save_solution(&mut game, "spot.pfs", &SaveOptions::compact(12))?;

// Reload. Counterfactual values are recomputed during this call.
let (mut game, memo) = load_solution("spot.pfs", &LoadOptions::default())?;

game.cache_normalized_weights();
let oop_equity = compute_average(&game.equity(0), game.normalized_weights(0));
```

`SaveOptions::navigable()` is the same thing without compression (always available, no
`zstd` feature needed). `SaveOptions::default()` equals `navigable()`.

## What loads immediately vs. recomputes

Choose the storage mode by how deep you need to navigate after loading.

| Saved mode | On disk | On load | Navigable streets after load |
|------------|---------|---------|------------------------------|
| **River** (default) | strategy only | recompute all counterfactual values (whole tree) | flop, turn, **river** |
| **Turn** | strategy + values, river nodes dropped | nothing (already complete) | flop, turn — river **panics** |
| **Flop** | strategy + values, turn & river dropped | nothing | flop — turn/river **panics** |

Notes:

- EV and equity always require `cache_normalized_weights()` first.
- River mode is the only mode that keeps the whole tree navigable. Its load cost is the
  recompute, not disk I/O — for large trees that recompute dominates load time.
- Turn/Flop physically drop deeper nodes; navigating into them hits the storage-mode guard
  in `play` and panics (`"Storage mode is not compatible"`). Use them only when you will
  never need deeper streets.

> [!NOTE]
> **Tournament-ICM solves are supported in every mode.** A game solved with
> `set_tournament_icm_config(...)` keeps its precomputed terminal ICM utilities in memory.
> Those large utilities are not written to disk, but the small `TournamentIcmConfig` is — so
> on load they are re-derived from the config + node arena (the precompute is pure and
> deterministic) before River's recompute runs. River, Turn, and Flop all reload ICM-correct
> values. (This requires the current file-format version; files written before it must be
> re-solved.)

## Choosing compression and precision

Two independent levers shrink the file:

| Lever | Set where | Effect |
|-------|-----------|--------|
| **Storage mode** | `SaveOptions::storage_mode` | River keeps everything navigable; Turn/Flop trade navigability for size. |
| **zstd level** | `SaveOptions::compression` | Whole-stream compression on top of whatever is stored. ~5x typical. |
| **Internal precision** | `allocate_memory(enable_compression)` — **not** a save option | `false` = 32-bit float; `true` = 16-bit quantized (≈ half the bytes) with a small EV error. |

Internal precision is fixed when memory is allocated and cannot be changed at save time —
the in-memory buffers already have that representation. To compare precisions you must
solve twice. See the gotcha below.

Representative numbers (small spot, flop `Ac9d2s`, 24bb; from `benches/compact_save.rs`):

| alloc | mode + zstd | file size | root EV error vs. 32-bit |
|-------|-------------|-----------|--------------------------|
| f32 | River, none | 24.1 MB | 0 (lossless) |
| f32 | River, L12 | 5.1 MB | 0 (lossless) |
| u16 | River, L12 | **3.1 MB** | 3.4e-2 chips |
| f32 | Turn, L12 | 0.35 MB | n/a (no river) |

Guidance: **solve in 32-bit float for accuracy, save River + zstd for the smallest fully
navigable artifact.** River f32 is lossless on every queryable value (chipEV, `bubble_factor`,
and tournament-ICM alike); u16 adds a small EV error (equity is unaffected). Reach for
Turn/Flop only to archive a street you will not revisit. The "lossless" numbers below are
chipEV.

## Gotcha: precision is chosen at allocation, not at save

```rust
game.allocate_memory(true);   // 16-bit quantized buffers — decided here
solve(&mut game, ...);
save_solution(&mut game, path, &SaveOptions::compact(12))?; // cannot "upgrade" to f32
```

`SaveOptions` deliberately has no precision field, so the API cannot imply a save-time
choice that does not exist. Cumulative regrets are likewise gone from a solved game (the
solver overwrites them while finalizing), so a compact save is queryable/recomputable but
**not** a resumable CFR checkpoint.

## File format (reference)

Unchanged from the existing serializer. Header (see `src/file.rs`):

- magic `0x09f15790`, version `1`
- compression byte: `0` none, `1` zstd
- data-type byte, varint estimated memory, memo string

The body is a bincode stream (whole-stream zstd when compressed, multithreaded with the
`rayon` feature). River mode writes only `storage1` (strategy) and zero-fills the
counterfactual buffers on load before recomputing them. `tree_config` (including
`bubble_factor`) and the `TournamentIcmConfig` are persisted; the large precomputed ICM
terminal utilities are not — they are re-derived from the config on load. Adding the ICM
config to the stream bumped the format version, so older files must be re-solved.

## Reproducing the measurements

```bash
cargo bench --bench compact_save --features zstd
```

Prints one table per spot (small / medium / multi-betsize) with on-disk size, save time,
load time, RAM at load, and the max per-hand equity/EV error vs. the 32-bit reference.

## Scope

This is a compact, navigable **final-solution** save. It is intentionally **not**:

- a resumable CFR checkpoint (no cumulative regrets / iteration state), or
- a partitioned multi-spot database container.

[`save_solution`]: ../src/solution.rs
[`load_solution`]: ../src/solution.rs
[`save_data_to_file`]: ../src/file.rs
[`load_data_from_file`]: ../src/file.rs
