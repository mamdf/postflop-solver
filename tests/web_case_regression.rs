extern crate postflop_solver;

mod support;

use support::web_case::{canonical_web_case_results, solve_web_case};

const MAX_COMBOS_DIFF: f32 = 1e-5;
const MAX_EQUITY_DIFF: f32 = 5e-4;
const MAX_EV_DIFF: f32 = 5e-3;
const MAX_ACTION_FREQ_DIFF: f32 = 5e-4;
const MAX_MEAN_EV_DIFF: f32 = 1e-3;
const MAX_MEAN_ACTION_FREQ_DIFF: f32 = 1e-4;

#[test]
fn regression_matches_canonical_web_case_csv() {
    let solved = solve_web_case();
    let canonical = canonical_web_case_results();

    assert_eq!(
        solved.hands.len(),
        canonical.len(),
        "solver/canonical hand count mismatch"
    );
    assert_eq!(
        canonical.len(),
        42,
        "canonical CSV should contain the 42 OOP combos for the web spot"
    );
    assert!(
        solved.final_exploitability <= solved.target_exploitability,
        "solver did not reach target exploitability: final={} target={}",
        solved.final_exploitability,
        solved.target_exploitability,
    );

    let mut total_ev_diff = 0.0;
    let mut total_action_freq_diff = 0.0;

    for hand in &solved.hands {
        let expected = canonical
            .get(&hand.hand)
            .unwrap_or_else(|| panic!("missing canonical row for {}", hand.hand));

        let combos_diff = (hand.combos - expected.combos).abs();
        let equity_diff = (hand.equity - expected.equity).abs();
        let ev_diff = (hand.ev - expected.ev).abs();
        let bet_diff = (hand.bet_freq - expected.bet_freq).abs();
        let check_diff = (hand.check_freq - expected.check_freq).abs();

        total_ev_diff += ev_diff;
        total_action_freq_diff += bet_diff + check_diff;

        assert!(
            combos_diff <= MAX_COMBOS_DIFF,
            "{} combos diff {} > {}",
            hand.hand,
            combos_diff,
            MAX_COMBOS_DIFF
        );
        assert!(
            equity_diff <= MAX_EQUITY_DIFF,
            "{} equity diff {} > {}",
            hand.hand,
            equity_diff,
            MAX_EQUITY_DIFF
        );
        assert!(
            ev_diff <= MAX_EV_DIFF,
            "{} EV diff {} > {}",
            hand.hand,
            ev_diff,
            MAX_EV_DIFF
        );
        assert!(
            bet_diff <= MAX_ACTION_FREQ_DIFF,
            "{} bet freq diff {} > {}",
            hand.hand,
            bet_diff,
            MAX_ACTION_FREQ_DIFF
        );
        assert!(
            check_diff <= MAX_ACTION_FREQ_DIFF,
            "{} check freq diff {} > {}",
            hand.hand,
            check_diff,
            MAX_ACTION_FREQ_DIFF
        );
    }

    let hand_count = solved.hands.len() as f32;
    let mean_ev_diff = total_ev_diff / hand_count;
    let mean_action_freq_diff = total_action_freq_diff / (hand_count * 2.0);

    assert!(
        mean_ev_diff <= MAX_MEAN_EV_DIFF,
        "mean EV diff {} > {}",
        mean_ev_diff,
        MAX_MEAN_EV_DIFF
    );
    assert!(
        mean_action_freq_diff <= MAX_MEAN_ACTION_FREQ_DIFF,
        "mean action freq diff {} > {}",
        mean_action_freq_diff,
        MAX_MEAN_ACTION_FREQ_DIFF,
    );
}
