use criterion::{Criterion, criterion_group, criterion_main};
use kagura_runtime::Runtime;
use kagura_testkit::CaptureHost;

const FIB: &str = include_str!("../../../examples/fib.ts");

fn fib(c: &mut Criterion) {
    c.bench_function("fib_10", |b| {
        b.iter(|| {
            let mut host = CaptureHost::default();
            let mut runtime = Runtime::default();
            runtime.run(FIB, &mut host).unwrap();
            std::hint::black_box(host.output());
        });
    });
}

criterion_group!(benches, fib);
criterion_main!(benches);
