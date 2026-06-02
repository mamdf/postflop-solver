extern crate postflop_solver;

// The shared web-case support module exposes helpers this binary does not use.
#[allow(dead_code)]
mod support;

use std::env;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;

use postflop_solver::{
    finalize, flop_from_str, load_solution, save_solution, ActionTree, BoardState, CardConfig,
    ExpectedValueUnit, LoadOptions, PostFlopGame, SaveOptions, TournamentIcmConfig, TreeConfig,
};
use support::web_case::{
    assert_fixture_matches_snapshot, canonical_web_case_spec, canonical_web_tree_fixtures,
    snapshot_for_fixture, solve_game, WEB_SNAPSHOT_DIFF_THRESHOLDS,
};

fn temp_path(tag: &str) -> PathBuf {
    env::temp_dir().join(format!("pfs_compact_it_{}_{tag}.bin", std::process::id()))
}

/// A River-mode compact save, once reloaded, must reproduce the published solution at
/// every node — including the deepest river nodes whose counterfactual values are
/// recomputed on load. This is the "navigate as if just computed" guarantee.
#[test]
fn river_save_reproduces_published_solution() {
    let spec = canonical_web_case_spec();
    let mut solved = solve_game(&spec);

    let path = temp_path("river_uncompressed");
    save_solution(&mut solved.game, &path, &SaveOptions::navigable()).unwrap();
    let (mut loaded, _memo) = load_solution(&path, &LoadOptions::default()).unwrap();
    fs::remove_file(&path).unwrap();

    for fixture in canonical_web_tree_fixtures() {
        assert_fixture_matches_snapshot(&mut loaded, &fixture, &WEB_SNAPSHOT_DIFF_THRESHOLDS);
    }
}

/// Same guarantee through the zstd-compressed path: compression must not change any
/// queryable value.
#[test]
#[cfg(feature = "zstd")]
fn compact_zstd_save_reproduces_published_solution() {
    let spec = canonical_web_case_spec();
    let mut solved = solve_game(&spec);

    let path = temp_path("river_zstd");
    save_solution(&mut solved.game, &path, &SaveOptions::compact(9)).unwrap();
    let (mut loaded, _memo) = load_solution(&path, &LoadOptions::default()).unwrap();
    fs::remove_file(&path).unwrap();

    for fixture in canonical_web_tree_fixtures() {
        assert_fixture_matches_snapshot(&mut loaded, &fixture, &WEB_SNAPSHOT_DIFF_THRESHOLDS);
    }
}

/// A Turn-mode save keeps flop and turn nodes fully queryable but physically drops the
/// river subtree: navigating into the river must be rejected (the river deal panics).
#[test]
fn turn_mode_save_drops_river_navigation() {
    let spec = canonical_web_case_spec();
    let mut solved = solve_game(&spec);

    let path = temp_path("turn_mode");
    let options = SaveOptions::navigable().with_storage_mode(BoardState::Turn);
    save_solution(&mut solved.game, &path, &options).unwrap();
    let (mut loaded, _memo) = load_solution(&path, &LoadOptions::default()).unwrap();
    fs::remove_file(&path).unwrap();

    let fixtures = canonical_web_tree_fixtures();

    // Flop and turn nodes still match the published solution.
    for name in [
        "root",
        "oop-bet-8",
        "oop-bet-8_ip-call_turn-kh",
        "oop-bet-8_ip-call_turn-kh_oop-check",
    ] {
        let fixture = fixtures
            .iter()
            .find(|fixture| fixture.name == name)
            .unwrap_or_else(|| panic!("missing fixture '{name}'"));
        assert_fixture_matches_snapshot(&mut loaded, fixture, &WEB_SNAPSHOT_DIFF_THRESHOLDS);
    }

    // Navigating into the river is not possible: the river deal hits the storage-mode
    // guard in `play` and panics.
    let river_fixture = fixtures
        .iter()
        .find(|fixture| fixture.name == "oop-bet-8_ip-call_turn-kh_check-check_river-5s")
        .expect("missing river fixture")
        .clone();

    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        snapshot_for_fixture(&mut loaded, &river_fixture);
    }));
    panic::set_hook(prev_hook);

    assert!(
        result.is_err(),
        "Turn-mode save must not allow navigating into the river"
    );
}

/// A River-mode save of a tournament-ICM solution must round-trip ICM-correct values: the
/// small config is persisted and the (dropped) terminal utilities are re-derived on load,
/// so the recompute uses ICM terminal values instead of falling back to chip EV.
#[test]
fn river_save_preserves_tournament_icm() {
    let lose_range = "KK-22,K9-K2,Q8-Q2,J8-J2,T8-T2,92+,82+,72+,62+";
    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), lose_range.parse().unwrap()],
        flop: flop_from_str("AcAdKh").unwrap(),
        ..Default::default()
    };
    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        ..Default::default()
    };
    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.set_tournament_icm_config(TournamentIcmConfig {
        stacks: vec![1000.0, 1000.0],
        payouts: vec![70, 30],
        oop_seat: 0,
        ip_seat: 1,
    })
    .unwrap();
    game.allocate_memory(false);
    finalize(&mut game);

    // Reference (ICM) values.
    assert!(game.uses_tournament_icm());
    assert_eq!(game.expected_value_unit(), ExpectedValueUnit::PayoutDelta);
    let reference_count = game.tournament_icm_utility_count();
    game.cache_normalized_weights();
    let reference_ev_oop = game.expected_values(0);
    let reference_ev_ip = game.expected_values(1);

    // River round-trip (the previously ICM-incorrect path).
    let path = temp_path("icm_river");
    save_solution(&mut game, &path, &SaveOptions::navigable()).unwrap();
    let (mut loaded, _memo) = load_solution(&path, &LoadOptions::default()).unwrap();
    fs::remove_file(&path).unwrap();

    // ICM config + utilities survived, and the recomputed values match the reference.
    assert!(
        loaded.uses_tournament_icm(),
        "ICM config must survive the round-trip"
    );
    assert_eq!(loaded.tournament_icm_utility_count(), reference_count);
    assert_eq!(loaded.expected_value_unit(), ExpectedValueUnit::PayoutDelta);

    loaded.cache_normalized_weights();
    let loaded_ev_oop = loaded.expected_values(0);
    let loaded_ev_ip = loaded.expected_values(1);

    let max_diff = |a: &[f32], b: &[f32]| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max)
    };
    assert!(
        max_diff(&reference_ev_oop, &loaded_ev_oop) < 1e-3,
        "OOP ICM EVs drifted after River round-trip"
    );
    assert!(
        max_diff(&reference_ev_ip, &loaded_ev_ip) < 1e-3,
        "IP ICM EVs drifted after River round-trip"
    );
}
