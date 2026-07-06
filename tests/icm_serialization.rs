extern crate postflop_solver;

use std::env;
use std::fs;
use std::path::PathBuf;

use postflop_solver::{
    card_from_str, flop_from_str, load_data_from_file, save_data_to_file, solve, ActionTree,
    BetSizeOptions, BoardState, CardConfig, PostFlopGame, TournamentIcmConfig, TreeConfig,
};

fn temp_path(tag: &str) -> PathBuf {
    env::temp_dir().join(format!("pfs_icm_ser_it_{}_{tag}.bin", std::process::id()))
}

/// The tiny river-only spot that produced `tests/fixtures/icm_legacy.pfs`. Keep this
/// construction in sync with the fixture: the legacy test compares against it.
///
/// Fixture provenance / regeneration: the fixture was written by the LEGACY encoder
/// (`VERSION_STR = "2026-06-02"`, commit 9477de5 or earlier), which did not persist
/// `base_contribution`. To regenerate it, check out such a revision and run, as an example
/// binary: build `reference_game()`, apply `reference_icm_config()` via
/// `set_tournament_icm_config_with_base_contribution(config, [30, 30])`, then
/// `allocate_memory(false)`, `solve(&mut game, 10, 0.0, false)`, and
/// `save_data_to_file(&game, "...", "tests/fixtures/icm_legacy.pfs", None)`.
/// It cannot be regenerated with the current encoder (which persists the base).
fn reference_game() -> PostFlopGame {
    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), "KK".parse().unwrap()],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: card_from_str("Qc").unwrap(),
        river: card_from_str("2s").unwrap(),
    };
    let bet_sizes = BetSizeOptions::try_from(("", "")).unwrap();
    let tree_config = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: 60,
        effective_stack: 970,
        rake_rate: 0.0,
        rake_cap: 0.0,
        bubble_factor: [1.0, 1.0],
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.20,
        merging_threshold: 0.1,
    };
    let action_tree = ActionTree::new(tree_config).unwrap();
    PostFlopGame::with_config(card_config, action_tree).unwrap()
}

fn reference_icm_config() -> TournamentIcmConfig {
    TournamentIcmConfig {
        stacks: vec![1000.0; 4],
        payouts: vec![50, 30, 20, 0],
        oop_seat: 0,
        ip_seat: 1,
    }
}

/// A re-rooted-style ICM game saved with a non-zero `base_contribution` must reload with
/// identical ICM terminal utilities: same exploitability scale and same reported EVs. The
/// legacy encoder (2026-06-02) dropped the base, so on old code the loaded game silently
/// fell back to `[0, 0]` and both values shifted; format 2026-07-06 persists the base.
// budget: ~1s @ RAYON_NUM_THREADS=4 (warm)
#[test]
fn icm_base_contribution_round_trips() {
    let mut game = reference_game();
    game.set_tournament_icm_config_with_base_contribution(reference_icm_config(), [30, 30])
        .unwrap();
    game.allocate_memory(false);
    solve(&mut game, 10, 0.0, false);

    let path = temp_path("base_round_trip");
    save_data_to_file(&game, "", &path, None).unwrap();
    let (mut loaded, _memo): (PostFlopGame, String) = load_data_from_file(&path, None).unwrap();
    fs::remove_file(&path).unwrap();

    assert!(
        loaded.uses_tournament_icm(),
        "ICM config must survive the round-trip"
    );
    assert!(
        (loaded.exploitability_target_scale() - game.exploitability_target_scale()).abs() < 1e-6,
        "exploitability scale drifted: base_contribution was not restored on load"
    );

    game.back_to_root();
    game.cache_normalized_weights();
    loaded.back_to_root();
    loaded.cache_normalized_weights();

    for player in 0..2 {
        let original = game.reported_expected_values(player);
        let restored = loaded.reported_expected_values(player);
        assert_eq!(original.len(), restored.len());
        let max_diff = original
            .iter()
            .zip(&restored)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_diff < 1e-6,
            "player {player} reported EVs drifted after round-trip (max diff {max_diff})"
        );
    }
}

/// A legacy-format file (encoder 2026-06-02, which did not persist `base_contribution`) must
/// still load, with `base_contribution = [0, 0]` — its historical behavior. The fixture was
/// written from the reference game configured with base `[30, 30]`; the legacy encoder
/// dropped the base, so the loaded game must match a zero-base game and must NOT match the
/// base-`[30, 30]` game (the limitation the 2026-07-06 format removes for new files).
// budget: ~1s @ RAYON_NUM_THREADS=4 (warm)
#[test]
fn legacy_icm_file_loads_with_zero_base() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/icm_legacy.pfs");
    let (legacy, _memo): (PostFlopGame, String) = load_data_from_file(fixture, None).unwrap();
    assert!(
        legacy.uses_tournament_icm(),
        "legacy ICM config must survive loading"
    );

    let mut zero_base = reference_game();
    zero_base
        .set_tournament_icm_config(reference_icm_config())
        .unwrap();

    let mut with_base = reference_game();
    with_base
        .set_tournament_icm_config_with_base_contribution(reference_icm_config(), [30, 30])
        .unwrap();

    let legacy_scale = legacy.exploitability_target_scale();
    assert!(
        (legacy_scale - zero_base.exploitability_target_scale()).abs() < 1e-6,
        "legacy file must keep its historical zero-base behavior"
    );
    assert!(
        (legacy_scale - with_base.exploitability_target_scale()).abs() > 1e-6,
        "zero-base and base-[30, 30] scales must differ for this test to be meaningful"
    );
}
