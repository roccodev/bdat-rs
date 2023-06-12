//! **NOTE**: Run with `cargo bench --features bench` to resolve test/bench-only modules

use bdat::io::legacy::scramble;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

pub fn criterion_benchmark(c: &mut Criterion) {
    use scramble::tests::{INPUT, KEY};

    c.bench_function("naive", |b| {
        b.iter(|| scramble::unscramble_naive(black_box(&mut INPUT), KEY))
    });
    c.bench_function("chunks", |b| {
        b.iter(|| scramble::unscramble_chunks(black_box(&mut INPUT), KEY))
    });
    c.bench_function("single", |b| {
        b.iter(|| scramble::unscramble_single(black_box(&mut INPUT), KEY))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
