extern crate postflop_solver;

mod support;

use std::collections::BTreeSet;
use std::env;
use std::fs;

use support::web_case::{
    assert_fixture_matches_snapshot, canonical_web_case_spec, canonical_web_fixture_summary,
    canonical_web_tree_fixtures, solve_game, WEB_SNAPSHOT_DIFF_THRESHOLDS,
};

#[test]
fn regression_matches_all_canonical_web_tree_fixtures() {
    let spec = canonical_web_case_spec();
    let mut solved = solve_game(&spec);
    let fixture_summary = canonical_web_fixture_summary();

    maybe_print_fixture_summary(&fixture_summary);

    // Este test resuelve el game canónico una vez y luego compara, fixture por fixture,
    // los snapshots del árbol contra sus CSV expected. Cada comparación valida por mano:
    // combos, equity, EV total, frecuencias por acción y EV por acción.

    assert!(
        solved.final_exploitability <= solved.target_exploitability,
        "solver did not reach target exploitability: final={} target={}\n\nCompared fixtures:\n{}",
        solved.final_exploitability,
        solved.target_exploitability,
        fixture_summary,
    );

    for fixture in canonical_web_tree_fixtures() {
        assert_fixture_matches_snapshot(&mut solved.game, &fixture, &WEB_SNAPSHOT_DIFF_THRESHOLDS);
    }
}

#[test]
fn canonical_web_fixture_registry_covers_all_active_csvs() {
    let registered_paths = canonical_web_tree_fixtures()
        .into_iter()
        .map(|fixture| fixture.csv_path.to_string())
        .collect::<BTreeSet<_>>();
    let active_paths = fs::read_dir("tests/fixtures/web")
        .unwrap_or_else(|error| panic!("failed to read tests/fixtures/web: {}", error))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("csv"))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<BTreeSet<_>>();

    assert_eq!(
        registered_paths, active_paths,
        "fixture registry must explicitly cover every active web CSV\n\nRegistered fixtures:\n{}",
        canonical_web_fixture_summary(),
    );
}

fn maybe_print_fixture_summary(summary: &str) {
    if env::var_os("POSTFLOP_SOLVER_SHOW_FIXTURE_SUMMARY").is_some() {
        println!(
            "\n[postflop-solver] Compared fixtures for canonical web regression:\n{}\n",
            summary
        );
    }
}
