use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use postflop_solver::solve;

#[path = "../tests/support/web_case.rs"]
mod web_case;

fn bench_canonical_web_case_solve(c: &mut Criterion) {
    let mut group = c.benchmark_group("canonical_web_case");
    group.sample_size(10);

    group.bench_function("solve", |b| {
        b.iter_batched(
            web_case::build_web_case_game,
            |mut game| {
                let exploitability = solve(
                    &mut game,
                    web_case::WEB_CASE_MAX_ITERATIONS,
                    web_case::web_case_target_exploitability(),
                    false,
                );
                black_box(exploitability);
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_canonical_web_case_solve);
criterion_main!(benches);
