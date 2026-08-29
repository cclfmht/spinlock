use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use spinlock::{McsLock, McsNode};
use std::sync::Mutex;
use std::thread;

const NUMS_THREADS: [u8; 5] = [1, 2, 4, 8, 16];
const NUM_ITERS_PER_THREAD: u32 = 10000;

fn mcs_shared_counter(num_thread: u8) {
    let mcs = McsLock::new(0);

    thread::scope(|s| {
        for _ in 0..num_thread {
            s.spawn(|| {
                let mut node = McsNode::new();
                for _ in 0..NUM_ITERS_PER_THREAD {
                    *mcs.lock(&mut node) += 1;
                }
            });
        }
    })
}

fn mutex_shared_counter(num_thread: u8) {
    let m = Mutex::new(0);

    thread::scope(|s| {
        for _ in 0..num_thread {
            s.spawn(|| {
                for _ in 0..NUM_ITERS_PER_THREAD {
                    *m.lock().unwrap() += 1;
                }
            });
        }
    })
}

fn bench_mcs_and_mutex(c: &mut Criterion) {
    for num_thread in NUMS_THREADS {
        let mut group = c.benchmark_group(format!(
            "MCS lock and Mutex on shared counter - {} thread(s)",
            num_thread
        ));
        group.bench_function(BenchmarkId::new("MCS lock", ""), |b| {
            b.iter(|| mcs_shared_counter(num_thread))
        });
        group.bench_function(BenchmarkId::new("Mutex", ""), |b| {
            b.iter(|| mutex_shared_counter(num_thread))
        });
        group.finish();
    }
}

criterion_group!(benches, bench_mcs_and_mutex);
criterion_main!(benches);
