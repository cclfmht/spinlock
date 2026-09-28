use core_affinity::{self, CoreId, get_core_ids, set_for_current};
use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, criterion_group, criterion_main};
use spinlock::{McsLock, McsNode};
use std::hint::black_box;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::{Arc, Barrier, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const NUMS_THREADS: [usize; 5] = [1, 2, 4, 8, 16];
/// Times of entering critical section in each iteration.
const NUM_CS_PER_ITERS: u32 = 100_000;

/// The locks implementing this trait can be benchmarked
trait BenchableLock: Send + Sync + 'static {
    const NAME: &'static str;
    type InitData;

    /// The lock name.
    fn name(&self) -> &'static str {
        Self::NAME
    }

    /// Initialization before starting work. This is primarily for MCS spinlock to initialize its
    /// nodes.
    fn init(&self) -> Self::InitData;

    /// Multiple threads will be assigned this work, and the time taken for all worker threads to
    /// finish this work will be measured. That is called "one iteration", from the perspective of
    /// Criterion. See its [guide](https://criterion-rs.github.io/book/analysis.html#measurement)
    /// for more information.
    fn work(&self, init_data: &mut Self::InitData);
}

impl BenchableLock for McsLock<i32> {
    const NAME: &'static str = "mcs";
    type InitData = McsNode;

    fn init(&self) -> Self::InitData {
        McsNode::new()
    }

    fn work(&self, init_data: &mut Self::InitData) {
        for _ in 0..NUM_CS_PER_ITERS {
            let mut g = self.lock(init_data);
            *g = black_box(*g + 1);
        }
    }
}

impl BenchableLock for Mutex<i32> {
    const NAME: &'static str = "mutex";
    type InitData = ();

    fn init(&self) -> Self::InitData {}

    fn work(&self, _init_data: &mut Self::InitData) {
        for _ in 0..NUM_CS_PER_ITERS {
            let mut g = self.lock().unwrap();
            *g = black_box(*g + 1);
        }
    }
}

/// Spawn a thread and pin it to a CPU core. The given routine `f` is only executed if we
/// successfully pinned the thread to the specified core.
fn spawn_on_core<F, T>(core_id: CoreId, f: F) -> JoinHandle<Option<T>>
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    thread::spawn(move || set_for_current(core_id).then(f))
}

fn bench_lock<'a, 'crit, L: BenchableLock>(
    lock: L,
    group: &'a mut BenchmarkGroup<'crit, WallTime>,
    n_threads: usize,
) {
    let lck = Arc::new(lock);
    let barrier_start = Arc::new(Barrier::new(n_threads + 1));
    let barrier_end = Arc::new(Barrier::new(n_threads + 1));
    let done = Arc::new(AtomicBool::new(false));
    let mut workers = Vec::with_capacity(n_threads);
    let core_ids = get_core_ids().unwrap();

    for core_id in &core_ids[..n_threads] {
        let lck_clone = Arc::clone(&lck);
        let barrier_start_clone = Arc::clone(&barrier_start);
        let barrier_end_clone = Arc::clone(&barrier_end);
        let done_clone = Arc::clone(&done);

        workers.push(spawn_on_core(*core_id, move || {
            let mut init_data = lck_clone.init();

            while !done_clone.load(Acquire) {
                barrier_start_clone.wait();
                lck_clone.work(&mut init_data);
                barrier_end_clone.wait();
            }
        }));
    }

    // Worker threads should all be stucked at the barriers at this point; otherwise, it didn't be
    // pinned on the specified CPU core successfully.
    if workers.iter().any(|h| h.is_finished()) {
        panic!("Failed to set CPU affinity");
    }

    group.bench_function(BenchmarkId::new(lck.name(), ""), |b| {
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
        bench_lock(McsLock::new(0), &mut group, num_threads);
        bench_lock(Mutex::new(0), &mut group, num_threads);
        group.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
