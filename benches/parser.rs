#![allow(missing_docs)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pysyn::printer::{dump, unparse, DumpOptions};

const SOURCE: &str = r#"
from collections import Counter

def classify(values: list[int]) -> dict[int, int]:
    counts = Counter(values)
    return {value: count for value, count in counts.items() if count > 1}

for value in range(10):
    print(classify([value, value + 1, value]))
"#;

fn benchmark_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("pysyn");
    group.bench_function("tokenize", |b| {
        b.iter(|| {
            let tokens = pysyn::lexer::tokenize(black_box(SOURCE)).collect::<Vec<_>>();
            black_box(tokens);
        });
    });
    group.bench_function("parse", |b| {
        b.iter(|| {
            let module = pysyn::parse_module(black_box(SOURCE)).expect("benchmark source is valid");
            black_box(module);
        });
    });
    group.bench_function("full", |b| {
        b.iter(|| {
            let module = pysyn::parse_module(black_box(SOURCE)).expect("benchmark source is valid");
            black_box(dump(&module, DumpOptions::default()));
            black_box(unparse(&module));
        });
    });
    group.finish();
}

criterion_group!(benches, benchmark_parser);
criterion_main!(benches);
