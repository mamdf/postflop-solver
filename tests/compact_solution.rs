extern crate postflop_solver;

// The shared web-case support module exposes helpers this binary does not use.
#[allow(dead_code)]
mod support;

use std::env;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;

use postflop_solver::{
    load_solution, save_solution, BoardState, LoadOptions, SaveOptions,
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
