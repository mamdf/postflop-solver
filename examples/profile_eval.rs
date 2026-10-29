// Evaluate a strategy profile of any solver with postflop-solver's best response.
//
//   cargo run --release --features profile-io --example profile_eval -- export <spec.json> <dir>
//   cargo run --release --features profile-io --example profile_eval -- eval <spec.json> <dir> [--scale S] [--fill N]
//
// `export` solves the spot and writes our own solution as a profile directory (a reference and a
// self-test: `eval` on it gives our own exploitability). `eval` locks the profile in `<dir>` (format
// in `src/profile/dir.rs`, v1 or v2) and prints one JSON line with the evaluation (EVs exclude the
// starting_pot / 2 bias). `--scale` overrides the amount scale (default: our pot / profile pot),
// `--fill N` runs N CFR iterations on the parts the profile does not cover.
//
// Spec JSON (only board, ranges, pot and stack are required):
//   {
//     "board": "Ks7h2h",                 // 3, 4 or 5 cards: the game starts on that street
//     "range_oop": "AA,KK,AK", "range_ip": "QQ-99,AQ",
//     "starting_pot": 100, "effective_stack": 200,
//     "rake_rate": 0.0, "rake_cap": 0.0,
//     "flop":  {"oop": ["50%", "60%"], "ip": ["50%", "60%"]},   // [bet sizes, raise sizes]
//     "turn":  {"oop": ["75%", "2.5x"], "ip": ["75%", "2.5x"]},
//     "river": {"oop": ["75%, a", "3x"], "ip": ["75%, a", "3x"]},
//     "turn_donk": null, "river_donk": "50%",
//     "add_allin_threshold": 1.5, "force_allin_threshold": 0.15, "merging_threshold": 0.1,
//     "max_iters": 1000, "target_pct": 0.3   // export: stop at this exploitability (% pot)
//   }
use postflop_solver::profile::dir::{read_dir, write_dir};
use postflop_solver::profile::*;
use postflop_solver::*;
use serde_json::{json, Value};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ret = match (args.first().map(String::as_str), args.len()) {
        (Some("export"), 3) => export(&args[1], &args[2]),
        (Some("eval"), 3..) => eval(&args[1], &args[2], &args[3..]),
        _ => Err("usage: profile_eval export <spec.json> <dir> | eval <spec.json> <dir> [--scale S] [--fill N]".into()),
    };
    if let Err(e) = ret {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn export(spec_path: &str, dir: &str) -> Result<(), String> {
    let t0 = Instant::now();
    let spec = load_spec(spec_path)?;
    let mut game = build_game(&spec)?;
    game.allocate_memory(false);
    let max_iters = spec["max_iters"].as_u64().unwrap_or(1000) as u32;
    let target = spec["target_pct"].as_f64().unwrap_or(0.3) as f32 / 100.0;
    let target = game.target_exploitability_from_fraction(target);
    let result = solve_with_result(&mut game, max_iters, target, false);
    let profile = StrategyProfile::from_game(&mut game, "postflop-solver")?;
    write_dir(&profile, dir)?;
    let scale = game.exploitability_target_scale();
    let out = json!({
        "iterations": result.num_iterations,
        "exploitability": result.exploitability,
        "exploitability_pct": 100.0 * result.exploitability / scale,
        "nodes": profile.nodes().len(),
        "seconds": t0.elapsed().as_secs_f64(),
    });
    println!("{out}");
    Ok(())
}

fn eval(spec_path: &str, dir: &str, flags: &[String]) -> Result<(), String> {
    let t0 = Instant::now();
    let mut opts = EvaluateOptions::default();
    for pair in flags.chunks(2) {
        let value = pair
            .get(1)
            .ok_or(format!("missing value for {}", pair[0]))?;
        match pair[0].as_str() {
            "--scale" => {
                opts.matching.amount_scale = Some(value.parse().map_err(|_| "bad --scale")?)
            }
            "--fill" => opts.fill_iterations = value.parse().map_err(|_| "bad --fill")?,
            flag => return Err(format!("unknown flag {flag}")),
        }
    }
    let spec = load_spec(spec_path)?;
    let profile = read_dir(dir)?;
    let mut game = build_game(&spec)?;
    game.allocate_memory(false);
    let ev = evaluate_source(&mut game, &profile, opts)?;
    let player = |p: &PlayerLockReport| {
        json!({"action_nodes": p.action_nodes, "locked": p.locked, "missing": p.missing,
               "mismatched": p.mismatched, "free_hands": p.free_hands, "unmapped_hands": p.unmapped_hands})
    };
    let out = json!({
        "source": profile.source,
        "exploitability": ev.exploitability,
        "exploitability_pct": ev.exploitability_pct,
        "best_response_ev": ev.best_response_ev,
        "profile_ev": ev.profile_ev,
        "complete": ev.complete,
        "report": {"oop": player(&ev.report.players[0]), "ip": player(&ev.report.players[1]),
                   "issues": ev.report.issues},
        "seconds": t0.elapsed().as_secs_f64(),
    });
    println!("{out}");
    if !ev.complete {
        eprintln!("warning: the profile does not cover the whole tree (see report)");
    }
    Ok(())
}

fn load_spec(path: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

fn build_game(spec: &Value) -> Result<PostFlopGame, String> {
    let str_of = |key: &str| spec[key].as_str().ok_or(format!("spec: missing {key:?}"));
    let num = |key: &str, default: f64| spec.get(key).and_then(Value::as_f64).unwrap_or(default);

    let board: String = str_of("board")?.split_whitespace().collect();
    let cards = (0..board.len())
        .step_by(2)
        .map(|i| card_from_str(board.get(i..i + 2).unwrap_or("")))
        .collect::<Result<Vec<_>, _>>()?;
    if !(3..=5).contains(&cards.len()) {
        return Err("spec: board must have 3, 4 or 5 cards".into());
    }
    let card_config = CardConfig {
        range: [str_of("range_oop")?.parse()?, str_of("range_ip")?.parse()?],
        flop: flop_from_str(&board[..6])?,
        turn: cards.get(3).copied().unwrap_or(NOT_DEALT),
        river: cards.get(4).copied().unwrap_or(NOT_DEALT),
    };

    let sizes = |street: &str| -> Result<[BetSizeOptions; 2], String> {
        let one = |player: &str| -> Result<BetSizeOptions, String> {
            let s = &spec[street][player];
            let (bet, raise) = (s[0].as_str().unwrap_or(""), s[1].as_str().unwrap_or(""));
            BetSizeOptions::try_from((bet, raise))
                .map_err(|e| format!("spec: {street}.{player}: {e}"))
        };
        Ok([one("oop")?, one("ip")?])
    };
    let donk = |key: &str| -> Result<Option<DonkSizeOptions>, String> {
        spec[key]
            .as_str()
            .map(DonkSizeOptions::try_from)
            .transpose()
    };
    let tree_config = TreeConfig {
        initial_state: [BoardState::Flop, BoardState::Turn, BoardState::River][cards.len() - 3],
        starting_pot: spec["starting_pot"]
            .as_i64()
            .ok_or("spec: missing \"starting_pot\"")? as i32,
        effective_stack: spec["effective_stack"]
            .as_i64()
            .ok_or("spec: missing \"effective_stack\"")? as i32,
        rake_rate: num("rake_rate", 0.0),
        rake_cap: num("rake_cap", 0.0),
        flop_bet_sizes: sizes("flop")?,
        turn_bet_sizes: sizes("turn")?,
        river_bet_sizes: sizes("river")?,
        turn_donk_sizes: donk("turn_donk")?,
        river_donk_sizes: donk("river_donk")?,
        add_allin_threshold: num("add_allin_threshold", 1.5),
        force_allin_threshold: num("force_allin_threshold", 0.15),
        merging_threshold: num("merging_threshold", 0.1),
        ..Default::default()
    };
    PostFlopGame::with_config(card_config, ActionTree::new(tree_config)?)
}
