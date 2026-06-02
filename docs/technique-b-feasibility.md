# Technique B — feasibility: drop a street, re-solve it on demand

**Verdict: feasible with caveats.** The solver already exposes — as first-class,
tested features — everything structural the technique needs: rooting a
`PostFlopGame` at a turn or river, extracting per-hand reach, and rebuilding input
ranges. The full flow exists end-to-end in [`examples/turn_resolve.rs`](../examples/turn_resolve.rs).
What is missing is **not solver capability** but **wiring** (a reusable reach
extractor, an on-demand re-solve injection point in the adapter, pot/stack
derivation). The one load-bearing unknown *was* **cost** — now **measured (Phase 4,
below)**: even the absolute ceiling (100%-vs-100% ranges = every possible river
combo, deep SPR, fat tree, single-thread, to 0.1% of pot) re-solves in **~100 ms**,
~10× under the 1 s budget; realistic ranges land at **4–17 ms**, bit-identical
across runs. The remaining risk is no longer cost — it is the three
silent-corruption footguns below.

This doc compares Technique B against what the crate does today, names the traps,
and lays out a code-free exploration plan whose first hard gate is a measurement.

---

## Two techniques, don't conflate them

| | **Technique A — strategy-only + value recompute** | **Technique B — drop-street + subgame re-solve** |
|---|---|---|
| Stores | the converged strategy of a street; drops its values | nothing for the dropped street; only structure + parent strategy |
| Reconstructs by | one deterministic CFV pass (`finalize()`) | re-solving the subgame with CFR |
| Determinism | deterministic | deterministic given pinned inputs, but a *different* equilibrium than the full game |
| Savings | moderate | large |
| Who | GTO+ river (Extensive); **this crate today** (River-mode compact save) | PioSOLVER `no_rivers` / `no_turns`; **the target here** |

The crate **already ships Technique A** (River-mode save: stores strategy,
recomputes CFVs at load via `finalize()`, plus optional lossy i16+f32 per-node
value compression). Technique B is the new capability.

---

## The "flop-only save" claim — what's true and what isn't

**True (confirmed in code):** a flop-only artifact (`BoardState::Flop`) physically
drops every turn and river node from the file (`src/game/serialization.rs:142-143`:
`num_nodes = num_nodes[0]`; deeper strategy bytes are never written). On load there
is **no automatic reconstruction** — navigating to a turn/river chance node trips an
explicit guard: `panic!("Storage mode is not compatible")`
(`src/game/interpreter.rs:284-290`). So recovering turn+river from a flop-only file
**is a from-scratch CFR re-solve**, not a deterministic recompute.

**The confusion to clear up:** the only "recompute" the library does at load is the
River-mode `finalize()` path (`src/game/serialization.rs:257-261`), which
deterministically rebuilds counterfactual **values** from a **retained** strategy.
That is *not* a re-solve, and it cannot help a flop-only file — a flop-only file has
no deep strategy to recompute from. Only CFR can fill that in.

**Why flop-only is the wrong target:** turn+river is the **bulk** of the tree.
Re-solving from the flop runs CFR over ~48 turn runouts, each fanning into ~44
rivers (thousands of full board subtrees) — essentially re-solving the entire
postflop tree. That destroys the "cheap recompute" premise.
`enable_parallelization` is `true` *above* the river precisely because that upper
fan-out is the wide, expensive part (`src/game/node.rs:201-204`).

**The cheap sweet spot is a TURN-level save + RIVER-only re-solve** (mirroring
PioSOLVER `no_rivers`). At a river node the board is **fixed** (1 runout, zero chance
nodes), and a turn-level save already retains the equilibrium turn strategy needed
to seed the river input ranges.

Qualitatively: river re-solve = 1 board, tiny; turn-from-flop = ~48 × tiny;
flop-to-river = ~48 × ~44 boards, huge.

---

## What already exists vs what's missing

| Capability | Exists today | Gap |
|---|---|---|
| Root a game at turn/river | ✅ `PostFlopGame::with_config` (`src/game/base.rs:147`); `check_card_config` derives `BoardState` (`base.rs:621-632`); tests `one_raise_all_range_with_turn/river` | constructor takes ranges *as given*; does not propagate parent reach |
| Extract per-hand reach | ✅ `weights(player)` (`src/game/interpreter.rs:560`); `play()` multiplies reach by the strategy row (`interpreter.rs:382-387`) | ✅ closed (Phase 1): `PostFlopGame::node_input_ranges` wraps it with a solved-gate + bunching guard |
| Rebuild input `Range` | ✅ `private_cards()` (`base.rs:444`) + `Range::from_hands_weights` (`src/range.rs:451`); `init_hands` re-derives weights with the board dead-mask (`base.rs:691-695`) | ✅ closed (Phase 1): `node_input_ranges` returns `[oop, ip]`, tested across an isomorphic chance hop |
| Derive subgame pot/stack | ✅ inputs via `total_bet_amount()` (`interpreter.rs:940`); derivation in `turn_resolve.rs:98-100` | no shared, tested helper — an off-by-one silently corrupts the subgame |
| Value recompute at load | ✅ `finalize()` (`serialization.rs:257-261`); sizing via `estimate_river_save_size` (`src/solution.rs:232`) | N/A — value recompute, not a re-solve |
| Adapter read-only navigation | ✅ `apply_navigation_steps` (`postflop-solver-api/.../spot_navigation.rs`) | **assumes every node is resident**; no branch detecting a non-resident river subtree and triggering an on-demand re-solve |

---

## The hard part: reconstructing the subgame's input ranges

Recovering the exact `(range_oop, range_ip)` entering a turn/river node is **possible
and non-theoretical** — `examples/turn_resolve.rs` does it end-to-end and is the
reference. The pattern:

1. Navigate to the node with `play()`. At a player node, `play()` multiplies the
   active player's reach vector by the chosen action's strategy row
   (`interpreter.rs:382-387`: `mul_slice(self.weights[player], strategy_row)`); at a
   chance node it calls `assign_zero_weights()` to drop board-blocked hands.
2. Read `weights(player)` (`interpreter.rs:560`): the live per-hand reach =
   initial weight × the running product of strategy probabilities along the line,
   with board-blocked hands = 0. **Use `weights()`, NOT `normalized_weights()`** —
   the latter folds the opponent's card-removal combinatorics into each hand
   (`interpreter.rs:499-502`) for display/equity, and seeding a subgame with it would
   double-count opponent mass inside your own range.
3. Zip `weights(player)` with `private_cards(player)` (same index order) and rebuild
   via `Range::from_hands_weights` (`range.rs:451`).
4. Pass the two `Range`s into a fresh subgame's `CardConfig.range`; `init_hands`
   re-derives `initial_weights` with the board dead-mask (`base.rs:691-695`).

The round-trip is **float-lossless** for a concrete, post-`finalize`, non-isomorphic
board: `set_weight_by_cards` is a direct f32 store (`range.rs:597`), and
`init_hands` only drops board-conflicting hands, so the two "live set" definitions
agree.

Pot/stack: `pot = starting_pot + oop_invested + ip_invested`;
`stack = effective_stack − max(invested)`, from `total_bet_amount()`
(`turn_resolve.rs:98-100`).

---

## Three silent-corruption footguns (each breaks *without* panicking)

1. **Non-finalized strategy.** `play()` weights with `self.strategy()`
   (`interpreter.rs:904`) = `normalized_strategy(node.strategy())`. `node.strategy()`
   holds the **average** strategy only **after** `finalize()` ran; before that it
   holds regret sums. If the seed comes from an artifact whose strategy was saved
   pre-finalize, **every reach vector — and every reconstructed range — is wrong but
   never panics.** → Hard gate: assert `Solved` state post-load and compare a known
   node's `strategy()` against the same node in the in-memory parent.

2. **Isomorphism un-swap — investigated and REFUTED for the standard (non-bunching)
   path (Phase 1).** The earlier hypothesis was that `weights()` is in a raw
   swapped-suit orientation and must be `apply_swap(reverse=true)`-ed before zipping
   with `private_cards()`. Verified against the code, it is **not**:
   `assign_zero_weights` runs on every chance hop and zeroes `self.weights[player]` by
   the *actual* board on the *same* `private_cards` indexing
   (`interpreter.rs:1123-1131`), so `weights()` is always index-aligned with
   `private_cards()`. `play()` only multiplies `weights` by `strategy()`, whose output
   is forward-swapped to match that orientation (`interpreter.rs:931-933` and `:386`);
   the `apply_swap(reverse=true)` dance touches only node strategy storage and
   temporary bunching buffers, never leaving `self.weights` persistently swapped
   (`interpreter.rs:512-519`, `:1146-1168`). Proven by the regression test
   `node_input_ranges_across_isomorphic_chance` (fires a real turn-suit isomorphism and
   asserts `weights` stays board-aligned). **The real caveat is bunching**, which uses a
   different removal layout (`bunching_arena`); `node_input_ranges` panics under
   bunching rather than guess.

3. **Coexistence contradiction.** A River-mode artifact (the default) runs
   `finalize()` at load and leaves the river fully navigable with its own retained
   equilibrium strategy. If you *also* inject an on-demand re-solve on the same board,
   you get **two equilibria for one node**: the parent's finalized strategy
   (consistent with the game above) and the isolated subgame's Nash. They will not
   match, and a cached re-solve persists the contradiction. → **Hard invariant: only
   re-solve nodes that are genuinely non-resident** (a turn-mode save dropped them).
   Never re-solve a node the parent can already serve.

---

## Determinism & consistency

**Run-to-run determinism: solid** given identical inputs and binary/target.
(1) No RNG in `src/` — it is full-tree Discounted CFR, not Monte-Carlo; every chance
child is enumerated (`solver.rs:98-121`). (2) Rayon does not reorder float sums:
parallel children write disjoint rows, and the reduction is single-threaded in fixed
ascending order (`src/sliceop.rs`), widened to f64. (3) DCFR discount factors are pure
functions of the iteration index.

**Two different "exactnesses" — keep them apart:**
- The **reach vector is exact** (float-lossless round-trip) for a concrete,
  post-finalize, non-isomorphic node.
- The **re-solved equilibrium is NOT full-game-exact**: it is the Nash of the
  *isolated* subgame given fixed input ranges (subgame resolving; see
  `turn_resolve.rs:11-17`). Self-consistent and deterministic, but not equal to what a
  full flop-to-river solve would have produced at that node. The API must label
  re-solved values as isolated-subgame results.

**Caching:** safe because deterministic, but (a) the stopping rule checks
exploitability every 10 iterations (`solver.rs:119-121`), so pin **both**
`max_num_iterations` and `target_exploitability` for reproducibility; (b) determinism
is per-binary — cross-architecture (different SIMD, wasm32 `simd128`) may differ at the
f32 bit level; (c) the cache key must be **provenance-explicit**:
`(board, parent-artifact identity / target hash, tree-config hash, pinned iters/target)`
— not just the derived input-range hash — so a hit provably maps to one parent
equilibrium.

---

## Cost / compression curve by street

`recompute` = on-demand CFR re-solve of the dropped streets.

- **Flop save (drops turn+river):** maximum compression, but **maximum** recompute —
  re-solving from the flop is CFR over the whole turn+river fan-out (~48 × ~44). Not
  cheap, not sub-second. **Wrong** choice for on-demand UX.
- **Turn save (drops river):** strong compression with cheap recompute — the
  **hypothesized sweet spot** (pending measurement). What's dropped is only the river
  subtrees; a river re-solve is a tiny action tree, zero chance nodes, two
  reach-weighted ranges on a fixed board. Mirrors PioSOLVER `no_rivers`. ⚠️ the disk
  win of a turn file *over the existing River-compact save* is **unmeasured** — size
  it with `estimate_river_save_size` (`solution.rs:232`) before claiming it.
- **River save (current default):** strategy-only, full tree navigable, CFVs
  recomputed at load via deterministic `finalize()` — a one-pass value recompute, no
  CFR, no on-demand re-solve. Largest on disk of the modes, cheapest "recompute".

**Why the turn is the knee — with the cost caveat:** saving shallower
(River→Turn→Flop) shrinks the file but the on-demand recompute cost explodes
super-linearly. But "river re-solve = sub-second" is a **hypothesis**:
- The river re-solve runs **single-thread** (`node.rs:202-204`:
  `enable_parallelization() = self.river == NOT_DEALT`; a river-rooted game has
  `river != NOT_DEALT` everywhere). Zero rayon speedup. The flop baseline is parallel;
  the river is not.
- The river subgame still spans **all** hand combos (`num_private_hands` ~1300+,
  `base.rs:31-32`); each DCFR iteration runs `regret_matching`/`fma_slices`/reach
  updates over `num_actions × num_hands` floats for both players, single-thread. "1
  board" ≠ little work — the width is in the hand matrix.
- Iterations-to-target were never quantified; the loop is bounded only by
  `max_num_iterations = 1000` (`solver.rs:119-121`). The existing bench
  (`benches/web_case.rs`) times a flop, not a river.

---

## Exploration plan (no code)

**Phase 1 — Found the reach reconstruction; two hard gates. ✅ DONE** (see
"Phase 1 results" above).
- **Gate (A) solved-state:** enforced — `node_input_ranges` panics unless `Solved`.
  (`node.strategy()` holds the cumulative strategy sum and `strategy()` normalizes it to
  the average on the fly; `finalize` — `utility.rs:245` — only computes CFVs and flips
  the state, so requiring `Solved` is the right, cheap guard.)
- **Gate (B) un-swap:** investigated and **refuted** for non-bunching games; covered by
  the `node_input_ranges_across_isomorphic_chance` regression test. Bunching is guarded
  out.

**Phase 2 — Map the adapter injection seam.**
Read `spot_navigation.rs`, `spot_solution.rs`, `engines/export.rs` in
`postflop-solver-api`. Decide where the "non-resident node → build+solve river subgame
→ serve from it" branch lives, *before* the chance `play()` that crosses into the
dropped street. Open question: build a **separate** river `PostFlopGame` (the
`turn_resolve.rs` pattern, clean) and have `export` read from it, vs splicing storage
back into the parent arena (fragile pointer bookkeeping — likely rejected).

**Phase 3 — Decide the artifact mode.**
Re-read `serialization.rs:142-146` (truncation) and `interpreter.rs:284-290` (the panic
proving a turn file cannot navigate the river). Open question: do we need a turn-mode
*file*, or is it cheaper to keep a navigable River-strategy-only save (already
recomputes CFVs via `finalize`) and re-solve the river only to add extra action-tree
detail? Resolve the coexistence rule here: never re-solve a node the parent can serve.

**Phase 4 — The measurement (hard go/no-go before any adapter wiring).** → see
[`examples/river_resolve_bench.rs`](../examples/river_resolve_bench.rs).
Build a flop with a realistic tree, walk to a concrete river, extract ranges, build a
river-only game with production-fat river sizings, and time `solve_with_result` at
0.5% / 0.3% / 0.1% of pot — single-thread, `--release`. Measure wall-clock,
`num_iterations`, and `memory_usage()`. Secondary: a turn re-solve from flop ranges for
the contrast. Success: **median river re-solve comfortably under ~1 s** at the
production target. If not, the cheap-recompute premise fails for our tree.

**Phase 5 — Determinism / caching policy.**
Confirm two identical re-solves match. Define the provenance-explicit cache key. Decide
whether the re-solve runs inside or outside the `SolveSession` mutex — a single-thread
river solve of a few hundred ms would block all session ops, so **a result cache turns
from optional to mandatory**. Document that served values are isolated-subgame Nash,
never mixed with parent finalized values on the same board.

---

## Phase 4 results (measured)

Source: [`examples/river_resolve_bench.rs`](../examples/river_resolve_bench.rs),
`cargo run --release`. Board `Td9d6h Qc 2s`. Fat river tree
`33%,75%,125%,allin + 2.5x raise` → root `[Check, Bet, Bet, Bet, AllIn]`. Every
**river** game runs **single-thread** (`enable_parallelization` is false once
`river != NOT_DEALT`); the flop/turn baselines run parallel. WSL2, release.

| Scenario | Hands/player | SPR | Target | Wall-clock (1 thread) | Iters |
|---|---|---|---|---|---|
| Real betting line (narrow ranges) | 104 | 1.29 | 0.1% pot | **4.3 ms** | 90 |
| Worst realistic (wide ranges, deep) | 167 | 6.7 | 0.1% pot | **16.6 ms** | 150 |
| **Theoretical max — 100% vs 100%** | **1081** = C(47,2) (every possible combo) | 6.7 | 0.1% pot | **103.5 ms** | 190 |
| _Turn re-solve (moderate tree, parallel)_ | _—_ | _—_ | _0.5% pot_ | _83 ms_ | _50_ |

**Determinism:** two identical re-solves returned bit-identical exploitability
(`0.204003`) and iteration counts. No RNG; rayon does not reorder float sums.

**Verdict — GO.** The river re-solve premise is confirmed with margin. 1081 hands is
the *absolute* maximum (47 cards left → C(47,2) = 1081; no wider river range exists),
so ~100 ms single-thread at 0.1% pot is the hard ceiling for this SPR/tree — ~10×
under the 1 s budget. Cost scales ~linearly with hand count here (167→1081 hands ≈
6.5×; 17→103 ms ≈ 6×), so the showdown O(n²) term is not dominating at these sizes.
The turn re-solve (moderate tree, *parallel*) already costs 83 ms — confirming the
**turn ≫ river** curve and that a flop-only save (which forces turn+river re-solve)
is the wrong target.

Caveats on the number: measured on one WSL2 host (10× headroom absorbs hardware
variance); these are *timing* runs. The correctness concerns (reach alignment under
isomorphism, the solved-state gate) are addressed separately in Phase 1, below.

## Phase 1 results (implemented)

Reach reconstruction is now a tested, first-class crate API rather than an
example-only helper.

- **`PostFlopGame::node_input_ranges(&self) -> [Range; 2]`** (`src/game/interpreter.rs`):
  returns the two players' input ranges entering the current node, ready to seed a
  re-rooted subgame. **Gate (A)** is enforced — it panics unless the game is `Solved`
  (the reach is only the equilibrium reach once the averaged strategy is in place) —
  and it panics under bunching rather than mis-handle the `bunching_arena` layout.
- **Gate (B) was REFUTED, not implemented.** The hypothesized isomorphism un-swap is
  unnecessary for non-bunching games: `weights()` is structurally kept index-aligned
  with `private_cards()` by `assign_zero_weights` on every chance hop (see footgun #2).
- **Regression test** `node_input_ranges_across_isomorphic_chance` (`src/game/tests.rs`):
  navigates flop check/check → an isomorphic turn (fires `turn_swap`) → river, and
  asserts a hand is zero in `weights()` **iff** it conflicts with the actual board —
  the property that would break if an un-swap were needed. The pre-existing
  `examples/turn_resolve.rs` only used a concrete non-isomorphic card, so this path was
  previously untested. Full suite: **49 passing**.

---

## Risks & limits

- **Unmeasured cost (risk #1).** "Sub-second river re-solve" is a hypothesis:
  single-thread over a ~1300×1300 hand matrix, iterations-to-target unquantified. If it
  fails, the turn-save/river-re-solve choice collapses.
- **Silent corruption from non-finalized strategy (risk #2).** Gate (A) is mandatory.
- **Isomorphism mis-zip.** Gate (B) is mandatory; `turn_resolve.rs` never exercises it.
- **Coexistence contradiction.** Mitigated by construction: re-solve only non-resident
  nodes.
- **Concurrency.** A re-solve inside the `SolveSession` mutex extends lock hold; either
  tolerate longer locks or run the re-solve outside the lock with a cache.
- **Pot/stack off-by-one** silently corrupts the subgame — needs a shared, tested helper.
- **Bunching** uses a different removal structure (`bunching_arena`); reconstruction is
  not self-contained — target the non-bunching path or handle it explicitly.
- **Cross-architecture** f32 differences (wasm32 vs native) — a deploy-consistency
  concern, not run-to-run.

---

## Recommendation

**GO to prototype.** Phase 4 — the hard go/no-go gate — **passed**: the worst possible
river re-solves single-thread in ~100 ms, ~10× under budget, and is deterministic. The
mechanics are sound and tested (`turn_resolve.rs`). Target **turn-mode save +
river-only on-demand re-solve** (not flop-only), re-solving only non-resident nodes.

With cost retired, the critical path is **correctness, not performance**:
1. ✅ **Gate (A) solved-state** — `node_input_ranges` panics unless the game is solved
   (Phase 1, done).
2. ✅ **Gate (B) isomorphism un-swap** — investigated and **refuted**: not needed for
   non-bunching games; `weights()` is structurally board-aligned (Phase 1, done, with a
   regression test).
3. ⬜ **Coexistence invariant** — re-solve only genuinely non-resident nodes (Phase 3).

Then the adapter wiring (Phase 2) and the mandatory result cache (Phase 5: a ~100 ms
solve inside the session mutex would block concurrent ops).
