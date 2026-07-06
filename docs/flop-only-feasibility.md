# Flop-only storage feasibility — per-line on-demand turn re-solve

**Verdict: GO for MTT-realistic spots (seam SPR ≤ ~2), with a conditional-GO
caveat for deep-SPR trees.** S1 (production-fat chipEV), S2 (wide-ceiling chipEV),
and S3 (TerminalIcm invested seam) pass every pre-registered criterion with
substantial margin. S4 (deep-SPR monster) lands in the pre-registered
conditional-GO band and is mitigable by cache-prewarm.

---

## 1. Framing — what changed since the rejection

`docs/technique-b-feasibility.md` §"The flop-only save claim" **rejected** flop-only
saves under the whole-fan-out framing: re-solving from the flop means running CFR
over ~48 turn runouts × ~44 rivers each — essentially re-solving the entire postflop
tree. That rejection stands for that framing.

The production use case is different. Auto-resolve river works per navigated line:
when a specific turn card is dealt, the adapter needs **one** turn-rooted subgame
for that card. That subgame contains **all of its rivers** (fully navigable to
showdown, Rayon-parallel across them). The per-line cost is therefore one
turn-subgame re-solve, not 48×44.

The second key property: **extraction at the pre-deal seam is turn-card-independent.**
`node_input_ranges()` and `node_pot_stack()` read state at the flop-level chance node
(board length 3), before any turn card is committed. One extraction seeds all ~45–48
possible turn subgames. This is prewarm-friendly: load the flop artifact once,
extract once, build and cache all turn subgames concurrently, and every subsequent
request is a cache hit.

Confirmed by the bench output (all spots):
```
Note: extraction is turn-card-independent — one extraction seeds all ~45-48 turn subgames.
```

---

## 2. Pre-registered GO/NO-GO criteria

These were decided **before** running the benchmark, based on the user-chosen
real-time budget for auto-resolve river.

| Criterion | Threshold | Basis |
|---|---|---|
| Disk win: flop vs turn | ≥ 5× (zstd) | Production file-size budget |
| chipEV turn re-solve, "total s" @ 0.1% pot | ≤ 1.5 s | Real-time UX budget |
| TerminalIcm turn re-solve, "total s" @ 0.02% parity target | ≤ 5 s | ICM convergence budget |
| Rivers resident in re-solved turn game | Required | No extra solver call per river |

**Conditional GO** (not outright GO or NO-GO): latency moderately above the chipEV
budget (1.5–5 s) with cache-prewarm as a named mitigation.

"total s" = build + allocate + solve wall time, as printed by the bench.

---

## 3. Measured results

**Date:** 2026-06-11.
**Host:** AMD Ryzen 5 5600X, 6c/12t, `rayon_threads=12`, full power (not capped), WSL2.
**Build:** release + `--features zstd`, zstd L12 for all compressed sizes.
**Source:** `examples/flop_storage_bench.rs`.
**Board (all spots):** `Td9d6h`, seam line check/bet/call.

S1–S3 numbers are from the definitive run (b1z0nth4q). S4 is from the heavy run
(b56pggw9q). Sub-second solve times vary ±~30% between runs on this host (WSL2
scheduling noise); the definitive run is the reference.

### S1 — production-fat chipEV

Parent: `Td9d6h`, pot 60, effective stack 145 (after the bet/call seam line: pot 100,
stack 125, SPR 1.25), flop sizings `"33%, 75%"` raise `"2.5x"`, turn+river sizings
`"33%, 75%, 125%, a"` raise `"2.5x"`. chipEV. 182 M storage elements, 1.470 GB RAM
uncompressed.

**Parent solve:** 35.9 s, 130 iters, expl 0.2728 (target 0.300).

**File sizes (zstd L12):**

| Mode | zstd bytes | unc bytes | save ms | turn/flop | river/turn | river/flop |
|---|---|---|---|---|---|---|
| River | 369 882 013 | 755 393 813 | 9 361 ms | — | 34.85× | 7 752× |
| Turn | 10 613 376 | 17 404 964 | 628 ms | — | — | — |
| Flop | 47 713 | 98 538 | 41 ms | 222.44× | — | — |

Flop artifact load: 50.1 ms.
Seam: pot=100 stack=125 SPR=1.25, OOP combos 27.6, IP combos 102.6.

**Turn re-solve (subgame shape: turn `"33%, 75%, 125%, a"` raise `"2.5x"`, river same):**

| target% | solve s | total s | iters | expl | mem MB |
|---|---|---|---|---|---|
| 0.50% | 0.167 | 0.172 | 40 | 0.44054 | 6.6 |
| 0.30% | 0.188 | 0.190 | 50 | 0.26310 | 6.6 |
| 0.10% | 0.333 | 0.335 | 90 | 0.09567 | 6.6 |

Turn-game RAM (future LRU entry cost): 6.6 MB.

PASS: rivers resident, determinism (A/B bit-identical, loaded==memory), round-trip
bit-equal ranges, guard panic confirmed.

### S2 — wide-ceiling chipEV (Range::ones())

Parent: both players `Range::ones()` (every combo weighted 1.0), single sizing `"50%"`,
stack 240, seam pot=120 stack=210 SPR=1.75. chipEV. 163 M storage elements,
1.296 GB RAM uncompressed.

**Parent solve:** 20.5 s, 90 iters, expl 0.2824 (target 0.300).

**File sizes (zstd L12):**

| Mode | zstd bytes | unc bytes | save ms | turn/flop | river/turn | river/flop |
|---|---|---|---|---|---|---|
| River | 160 831 421 | 655 879 319 | 4 195 ms | — | 24.60× | 3 541× |
| Turn | 6 537 165 | 15 052 504 | 431 ms | — | — | — |
| Flop | 45 415 | 106 226 | 40 ms | 143.94× | — | — |

Flop artifact load: 241.1 ms.
Seam: pot=120 stack=210 SPR=1.75, OOP combos 486.4, IP combos 773.3.

**Turn re-solve (subgame shape: turn `"50%"` no raise, river `"50%"`):**

| target% | solve s | total s | iters | expl | mem MB |
|---|---|---|---|---|---|
| 0.50% | 0.230 | 0.240 | 50 | 0.38349 | 12.2 |
| 0.10% | 0.401 | 0.411 | 100 | 0.11739 | 12.2 |

Turn-game RAM: 12.2 MB.

PASS: all verifications.

S2 is the upper-bound on range width (no wider hands exist). At 0.1% it re-solves in
0.411 s total, 3.7× inside the 1.5 s budget.

### S3 — TerminalIcm invested seam

Parent: S1-style ranges, `TerminalIcm` stacks `[35, 35, 70]`, payouts `[60, 40, 0]`,
seats 0/1 (mirroring the API parity test in `postflop-solver-api/src/store/mod.rs:740–766`).
Moderate parent tree. ICM base: `base_contribution = [10, 10]` (pot=40 including the
invested amounts). Seam stack=25, SPR=0.62.

**Parent solve:** 8.6 s, 240 iters, expl 0.002024 (target 0.002131). `ICM=true`.

**File sizes (zstd L12):**

| Mode | zstd bytes | unc bytes | save ms | turn/flop | river/turn | river/flop |
|---|---|---|---|---|---|---|
| River | 19 587 589 | 61 813 638 | 904 ms | — | 14.99× | 1 155× |
| Turn | 1 306 623 | 2 451 463 | 108 ms | — | — | — |
| Flop | 16 964 | 42 207 | 39 ms | 77.02× | — | — |

Flop artifact load: 44.4 ms.
Seam: pot=40 stack=25 SPR=0.62, OOP combos 33.8, IP combos 69.1.

**Turn re-solve (same fat tree, TerminalIcm):**

| target% | solve s | total s | iters | expl | mem MB |
|---|---|---|---|---|---|
| 0.50% | 0.099 | 0.100 | 40 | 0.04871 | 1.8 |
| 0.10% | 0.132 | 0.133 | 60 | 0.02013 | 1.8 |
| 0.02% | 0.313 | 0.314 | 140 | 0.00405 | 1.8 |

Turn-game RAM: 1.8 MB. ICM convergence at 0.02% parity target: 140 iters, 0.314 s.

PASS: all verifications including `S3 seam base_contribution [10, 10] > 0 (ICM offset
is load-bearing)`.

### S4 — deep-SPR monster (FLOP_BENCH_HEAVY)

Parent: fat sizings everywhere, stack 400, seam stack=380, SPR=3.80. chipEV. 786 M
storage elements, 6.251 GB RAM uncompressed. Single target measured (0.1%).

**Parent solve:** 836.4 s, 830 iters, expl 0.0581 (target 0.060). Uncompressed files
skipped (disabled for this spot).

**File sizes (zstd L12, no uncompressed):**

| Mode | zstd bytes | save ms | turn/flop | river/turn | river/flop |
|---|---|---|---|---|---|
| River | 1 650 482 837 | 30 596 ms | — | 51.07× | 15 558× |
| Turn | 32 319 971 | 885 ms | — | — | — |
| Flop | 106 085 | 44 ms | 304.66× | — | — |

Flop artifact load: 48.8 ms.
Seam: pot=100 stack=380 SPR=3.80, OOP combos 53.2, IP combos 111.4.

**Turn re-solve (fat tree, chipEV):**

| target% | solve s | total s | iters | expl | mem MB |
|---|---|---|---|---|---|
| 0.10% | 3.004 | 3.011 | 440 | 0.09422 | 28.2 |

Turn-game RAM: 28.2 MB.

PASS: all verifications.

3.0 s total at 0.1% exceeds the 1.5 s criterion — this is the **conditional-GO** case.
See §5 for mitigations.

---

## 4. ICM notes

- **`restore_tournament_icm_from_config` over a flop-truncated arena works without
  modification.** The contingent engine fix feared in the plan (analogous to the Turn
  mode fix in commit `4906bbf`) was **not needed**. S3 loaded, ICM config was restored,
  and `uses_tournament_icm()` returned `true` on the loaded artifact — confirmed by
  `PASS [S3-terminal-icm] loaded uses_tournament_icm() matches cfg`.

- **Seam `base_contribution` is asserted > 0.** The S3 seam has `base_contribution =
  [10, 10]` (both players invested before the seam). The bench asserts this is non-zero
  before building the turn subgame, mirroring the API parity test's invested-seam
  requirement (`postflop-solver-api/src/store/mod.rs:740–766`). PASS.

- **ICM convergence is fast on this subgame.** The 0.02% parity target required only
  140 iters (0.314 s on the definitive run). The API parity test ceiling is
  `max_iterations = 3000`; this subgame is far from that bound.

- **Re-measured 2026-07-06 under pot-equivalent ICM targets.** Commit `9477de5`
  changed `exploitability_target_scale()` in ICM mode from the bust-dominated max
  terminal delta to the ICM value of the starting pot, so a given `target%` is now
  pot-comparable across chipEV and ICM. For the S3 parent (flop root) the same 0.1%
  fraction became ~2.8× tighter (target 0.006005 → 0.002131; 140 → 240 iters,
  5.3 → 8.6 s). The low-SPR turn subgame barely moved (130 → 140 iters @ 0.02%):
  at SPR 0.62 the all-in swing was never much larger than the pot swing. All S3
  numbers in this doc are from the 2026-07-06 re-run, which also exercised the
  `base_contribution`-persisting file format of commit `f99bc16` (all bit-equality
  and determinism checks passed on the new format).

- **The re-solved equilibrium is the isolated-subgame Nash**, not the full-game
  equilibrium. This is the same approximation already accepted for Technique B river
  re-solves — see `turn_resolve.rs:11–17` for the explicit caveat. ICM convergence
  measures exploitability within the subgame, not versus the parent tree.

---

## 5. Verdict

### GO criteria cross-check

| Spot | Disk win turn/flop (zstd) | total s @ 0.1% | Disk GO? | Latency GO? |
|---|---|---|---|---|
| S1 production-fat | 222× | 0.335 s | YES (44× margin) | YES (4.5× margin) |
| S2 wide-ceiling | 144× | 0.411 s | YES (29× margin) | YES (3.7× margin) |
| S3 TerminalIcm | 77× | 0.133 s @ 0.1%; 0.314 s @ 0.02% (ICM criterion) | YES (15× margin) | YES (16× margin vs 5 s) |
| S4 monster | 305× | 3.011 s | YES (61× margin) | **Conditional GO** |

All spots clear the ≥ 5× disk criterion by at least a 15× additional margin.
S1–S3 clear the ≤ 1.5 s latency criterion. S4 at SPR 3.80 does not.

**Save-time bonus.** The flop artifact is also dramatically cheaper to write than
river or turn: S1 flop saves in ~41 ms vs river 9 361 ms and turn 628 ms. S4 flop
saves in ~44 ms vs river 30 596 ms. This is a production concern too — the solver
write path is synchronous and blocks the session.

### S4 conditional-GO mitigations

1. **Cache-prewarm at artifact load.** Extraction at the seam is turn-card-independent.
   Load the flop artifact once, extract the seam ranges once, then build all ~45–48
   turn subgames eagerly in the background. Every subsequent request is a cache hit
   (~0 solve latency). This is feasible because extraction costs ~50 ms (flop load)
   and the subgames can be built in parallel.

2. **Per-request target relaxation.** At 0.5% pot the turn re-solve for a deep-SPR
   spot is expected to be well under 1 s (extrapolating from S1's 0.172 s at 0.5%).
   For real-time UX at deep SPR, 0.5% pot is often acceptable.

**For MTT-realistic spots (seam SPR ≤ ~2):** S1 (SPR 1.25) and S2 (SPR 1.75) show
the real-time budget holds with margin. S3 (SPR 0.62, ICM) passes decisively.
Deep-stack cash game spots (SPR ≥ 3) are conditional-GO only.

---

## 6. Caveats

- **One host, one board.** All measurements are from AMD Ryzen 5 5600X (6c/12t,
  rayon_threads=12), WSL2, release build with zstd. A single board texture
  (`Td9d6h`). Production hardware variance should be estimated against this baseline;
  the pre-registered budgets include ~4× headroom on S1–S3.

- **Single seam line.** The measured seam line is check/bet/call at the flop root.
  Other seam lines (bet/raise, longer sequences) affect the seam pot/stack and range
  width but not the structural properties measured here.

- **Isolated-subgame Nash.** The re-solved equilibrium is the Nash of the isolated
  turn subgame given the fixed extracted input ranges. This is the same approximation
  already accepted for Technique B river re-solves (`turn_resolve.rs:11–17`). It is
  not the full flop-to-river equilibrium evaluated at that node.

- **HTTP/adapter overhead unmeasured.** The "total s" columns are solve-path only
  (build + allocate + solve). Adapter serialization, HTTP framing, and client RTT
  are street-independent and not included here.

- **Sub-second timing variance ~±30%.** Sub-second solve times vary between runs on
  this host (WSL2 scheduling). Definitive run (b1z0nth4q) is the reference. The
  conditional-GO boundary for S4 (3.0 s) is far enough above 1.5 s that this
  variance does not flip the verdict.

---

## 7. Follow-up scope (out of scope here — wiring into production)

If this change proceeds to adapter wiring, the following are required:

1. **Turn-seam detection.** Generalize `is_dropped_river_deal`
   (`postflop-solver-api/src/application/spot_navigation.rs:321`) to also detect a
   flop-mode artifact reaching a turn-deal chance node (`storage_mode() == Flop` and
   `current_board().len() == 3`). The existing river-deal detection
   (`storage_mode() == Turn` and `current_board().len() == 4`) covers Technique B;
   flop-mode adds a second case.

2. **`resolve_turn_subgame`.** Mirror `resolve_river_subgame`
   (`postflop-solver-api/src/engines/postflop_solver.rs:262`): accept the seam
   pot/stack, two extracted `Range`s, a turn card, and a tree config; build a
   turn-rooted `PostFlopGame` (`CardConfig { turn: turn_card, river: NOT_DEALT }`,
   `initial_state: BoardState::Turn`), call `set_tournament_icm_config_with_base_contribution`
   when ICM, allocate, solve, and return. **Do not use the `turn_resolve.rs` extraction
   pattern** (it extracts AFTER `play(turn_card)`, which panics on a flop-mode artifact);
   extract at board-length-3 on the pre-deal chance node.

3. **Turn-game LRU.** Per-card turn subgame costs: S1 6.6 MB, S2 12.2 MB, S3 1.8 MB,
   S4 28.2 MB. A full prewarm of ~46 turn subgames for S1 is ~304 MB; for S4 ~1.3 GB.
   LRU sizing must account for the full prewarm footprint per open session.

4. **Chained river serving.** Once inside a resolved turn game, rivers are resident and
   fully navigable. River action requests downstream of the turn-deal step replay on the
   **separate** turn `PostFlopGame` without additional solver calls.

5. **`spot_hash` ignores `storage_mode`.** The same spot solved at different storage
   modes upserts one row in the DB, because `CanonicalSpotKey` does not include
   `storage_mode`
   (`postflop-solver-api/src/store/hash.rs`). A flop-mode and a turn-mode solve of the
   same spot would alias. Policy decision required before wiring: either add
   `storage_mode` to the hash key, or enforce that only one mode is written per spot.
