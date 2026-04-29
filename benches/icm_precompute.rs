use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use postflop_solver::*;

const FLOP: &str = "Ac9d2s";
const OOP_RANGE: &str = "AA-TT,AKs-AQs,AKo";
const IP_RANGE: &str = "JJ-66,AQs-AJs,KQs-KJs,QJs";
const STARTING_POT: i32 = 12;
const EFFECTIVE_STACK: i32 = 24;
const BET_SIZES: (&str, &str) = ("65%", "65%");

fn build_unallocated_web_case() -> PostFlopGame {
    let bet_sizes: BetSizeOptions = BetSizeOptions::try_from(BET_SIZES).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: STARTING_POT,
        effective_stack: EFFECTIVE_STACK,
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.20,
        merging_threshold: 0.1,
        ..Default::default()
    };

    let card_config = CardConfig {
        range: [OOP_RANGE.parse().unwrap(), IP_RANGE.parse().unwrap()],
        flop: flop_from_str(FLOP).unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    PostFlopGame::with_config(card_config, action_tree).unwrap()
}

fn bench_icm_precompute(c: &mut Criterion) {
    let mut group = c.benchmark_group("icm_precompute");
    group.sample_size(10);

    group.bench_function("canonical_web_case", |b| {
        b.iter_batched(
            build_unallocated_web_case,
            |mut game| {
                let count = game
                    .set_tournament_icm_config(TournamentIcmConfig {
                        stacks: vec![1900.0, 1640.0, 1640.0, 1900.0],
                        payouts: vec![50, 30, 20],
                        oop_seat: 0,
                        ip_seat: 1,
                    })
                    .unwrap();
                black_box(count);
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_icm_precompute);
criterion_main!(benches);
