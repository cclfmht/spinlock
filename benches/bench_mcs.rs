use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, criterion_group, criterion_main};
use spinlock::{McsLock, McsNode};
use std::hint::black_box;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const NUMS_THREADS: [usize; 5] = [1, 2, 4, 8, 16];
/// Times of entering critical section in each iteration.
const NUM_CS_PER_ITERS: u32 = 100_000;

fn bench_mcs<'a, 'crit>(group: &'a mut BenchmarkGroup<'crit, WallTime>, n_threads: usize) {
    let mcs = Arc::new(McsLock::new(0));
    let barrier_start = Arc::new(Barrier::new(n_threads + 1));
    let barrier_end = Arc::new(Barrier::new(n_threads + 1));
    let done = Arc::new(AtomicBool::new(false));
    let mut workers = Vec::with_capacity(n_threads);

    for _ in 0..n_threads {
        let mcs_clone = Arc::clone(&mcs);
        let barrier_start_clone = Arc::clone(&barrier_start);
        let barrier_end_clone = Arc::clone(&barrier_end);
        let done_clone = Arc::clone(&done);

        workers.push(thread::spawn(move || {
            let mut node = McsNode::new();

            while !done_clone.load(Acquire) {
                barrier_start_clone.wait();
                for _ in 0..NUM_CS_PER_ITERS {
                    let mut g = mcs_clone.lock(&mut node);
                    *g = black_box(*g + 1);
                }
                barrier_end_clone.wait();
            }
        }));
    }

    group.bench_function(BenchmarkId::new("mcs", ""), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                barrier_start.wait();
                let start = Instant::now();
                barrier_end.wait();
                total += start.elapsed();
            }
            total
        })
    });

    // Notify the end of benchmark. This causes the worker threads to terminate.
    done.store(true, Release);
}

fn bench_mutex<'a, 'crit>(group: &'a mut BenchmarkGroup<'crit, WallTime>, n_threads: usize) {
    let mutex = Arc::new(Mutex::new(0));
    let barrier_start = Arc::new(Barrier::new(n_threads + 1));
    let barrier_end = Arc::new(Barrier::new(n_threads + 1));
    let done = Arc::new(AtomicBool::new(false));
    let mut workers = Vec::with_capacity(n_threads);

    for _ in 0..n_threads {
        let mutex_clone = Arc::clone(&mutex);
        let barrier_start_clone = Arc::clone(&barrier_start);
        let barrier_end_clone = Arc::clone(&barrier_end);
        let done_clone = Arc::clone(&done);

        workers.push(thread::spawn(move || {
            while !done_clone.load(Acquire) {
                barrier_start_clone.wait();
                for _ in 0..NUM_CS_PER_ITERS {
                    let mut g = mutex_clone.lock().unwrap();
                    *g = black_box(*g + 1);
                }
                barrier_end_clone.wait();
            }
        }));
    }

    group.bench_function(BenchmarkId::new("mutex", ""), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                barrier_start.wait();
                let start = Instant::now();
                barrier_end.wait();
                total += start.elapsed();
            }
            total
        })
    });

    // Notify the end of benchmark. This causes the worker threads to terminate.
    done.store(true, Release);
}

fn bench(c: &mut Criterion) {
    for num_threads in NUMS_THREADS {
        let mut group = c.benchmark_group(format!("{num_threads} threads"));
        bench_mcs(&mut group, num_threads);
        bench_mutex(&mut group, num_threads);
        group.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
