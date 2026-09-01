use spinlock::{McsLock, McsNode};
use std::thread;

#[test]
fn shared_counter_1() {
    let sp = McsLock::new(0);

    thread::scope(|s| {
        s.spawn(|| {
            let mut node = McsNode::new();
            *sp.lock(&mut node) += 1;
        });
        s.spawn(|| {
            let mut node = McsNode::new();
            let mut g = sp.lock(&mut node);
            *g += 2;
        });
    });

    let mut node = McsNode::new();
    let g = sp.lock(&mut node);
    assert_eq!(*g, 3);
}

#[test]
fn shared_vec_1() {
    let sp = McsLock::new(Vec::new());

    thread::scope(|s| {
        s.spawn(|| {
            let mut node = McsNode::new();
            sp.lock(&mut node).push(String::from("one"))
        });
        s.spawn(|| {
            let mut node = McsNode::new();
            let mut g = sp.lock(&mut node);
            g.push(String::from("two"));
            g.push(String::from("two"));
        });
    });

    let mut node = McsNode::new();
    let g = sp.lock(&mut node);
    let result1 = [
        String::from("one"),
        String::from("two"),
        String::from("two"),
    ];
    let result2 = [
        String::from("two"),
        String::from("two"),
        String::from("one"),
    ];
    assert!(g.as_slice() == &result1 || g.as_slice() == &result2);
}

#[test]
fn shared_vec_2() {
    let sp = McsLock::new(Vec::new());

    thread::scope(|s| {
        s.spawn(|| {
            let mut node = McsNode::new();
            let mut g = sp.lock(&mut node);
            g.push("Rust");
            g.push("C");
        });
        s.spawn(|| {
            let mut node = McsNode::new();
            let mut g = sp.lock(&mut node);
            g.push("apple");
            g.push("banana");
            g.push("orange");
        });
    });

    let mut node = McsNode::new();
    let g = sp.lock(&mut node);
    let result1 = ["Rust", "C", "apple", "banana", "orange"];
    let result2 = ["apple", "banana", "orange", "Rust", "C"];
    assert!(g.as_slice() == &result1 || g.as_slice() == &result2);
}

#[test]
fn shared_counter_2() {
    let sp = McsLock::new(0);
    let num_threads = 1000;

    thread::scope(|s| {
        for _ in 0..num_threads {
            s.spawn(|| {
                let mut node = McsNode::new();
                *sp.lock(&mut node) += 1
            });
        }
    });

    let mut node = McsNode::new();
    let g = sp.lock(&mut node);
    assert_eq!(*g, num_threads);
}

#[test]
fn shared_counter_3() {
    let sp = McsLock::new(0);

    thread::scope(|s| {
        s.spawn(|| {
            let mut node = McsNode::new();
            for _ in 0..10000 {
                *sp.lock(&mut node) += 1;
            }
        });
        s.spawn(|| {
            let mut node = McsNode::new();
            for _ in 0..10000 {
                *sp.lock(&mut node) -= 1;
            }
        });
    });

    let mut node = McsNode::new();
    let g = sp.lock(&mut node);
    assert_eq!(*g, 0);
}
