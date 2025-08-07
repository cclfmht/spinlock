#![allow(unused_imports)]

use spinlock::{McsLock, McsNode};
use std::sync::atomic::{AtomicU64, Ordering::*};
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    let n = 1000;
    let lock = McsLock::new(0u32);
    let race_time = AtomicU64::new(0);

    thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                let begin = Instant::now();

                let mut node = McsNode::new();
                *lock.lock(&mut node) += 1;

                race_time.fetch_add(begin.elapsed().as_nanos() as u64, Relaxed);
            });
        }
    });

    println!("{} ns", race_time.load(Relaxed));
}
