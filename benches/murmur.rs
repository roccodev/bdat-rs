use bdat::hash::murmur3_str;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("hex256", |b| {
        b.iter(|| murmur3_str(black_box("0c3ce0caf5997fb777d2e519dfad77c9ff24256bed90")))
    });
}

criterion_group!(murmur_bench, criterion_benchmark);
criterion_main!(murmur_bench);
