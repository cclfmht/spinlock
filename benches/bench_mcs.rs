use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use spinlock::{McsLock, McsNode};
use std::sync::Mutex;
use std::thread;

const NUM_THREADS: u8 = 16;
const NUM_ITERS_PER_THREAD: u32 = 10000;

fn mcs_shared_counter() {
    let mcs = McsLock::new(0);

    thread::scope(|s| {
        for _ in 0..NUM_THREADS {
            s.spawn(|| {
                let mut node = McsNode::new();
                for _ in 0..NUM_ITERS_PER_THREAD {
                    *mcs.lock(&mut node) += 1;
                }
            });
        }
    })
}

fn mutex_shared_counter() {
    let m = Mutex::new(0);

    thread::scope(|s| {
        for _ in 0..NUM_THREADS {
            s.spawn(|| {
                for _ in 0..NUM_ITERS_PER_THREAD {
                    *m.lock().unwrap() += 1;
                }
            });
        }
    })
}

fn bench_mcs_and_mutex(c: &mut Criterion) {
    let mut group = c.benchmark_group("MCS lock and Mutex");
    group.bench_function(BenchmarkId::new("MCS lock on shared counter", 0), |b| {
        b.iter(|| mcs_shared_counter())
    });
    group.bench_function(BenchmarkId::new("Mutex on shared counter", 1), |b| {
        b.iter(|| mutex_shared_counter())
    });
    group.finish();
}

criterion_group!(benches, bench_mcs_and_mutex);
criterion_main!(benches);
