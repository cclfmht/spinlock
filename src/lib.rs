//! A simple MCS spin lock implementation

use std::cell::UnsafeCell;
use std::hint;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering::*, fence};

pub struct McsNode {
    next: AtomicPtr<McsNode>,
    locked: AtomicBool,
}

impl McsNode {
    pub fn new() -> Self {
        Self {
            next: AtomicPtr::new(ptr::null_mut()),
            locked: AtomicBool::new(false),
        }
    }
}

pub struct McsLock<T: ?Sized> {
    tail: AtomicPtr<McsNode>,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send + ?Sized> Sync for McsLock<T> {}

impl<T> McsLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            tail: AtomicPtr::new(ptr::null_mut()),
            value: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> McsLock<T> {
    pub fn lock<'a, 'b>(&'a self, node: &'b mut McsNode) -> McsLockGuard<'a, 'b, T> {
        let prev = self.tail.swap(node, Acquire);

        if !prev.is_null() {
            // SAFETY: `prev` is obviously non-null.
            unsafe {
                (*prev).next.store(node, Relaxed);
            }
            // spinning
            Self::lock_contended(node);
            // At this point, it's our turn to use the lock. Since we only
            // use `Relaxed` order when spinning, put an `Acquire` fence here
            // to synchronize with the release-store in McsLockGuard::drop().
            fence(Acquire);
        }
        McsLockGuard::new(self, node)
    }

    #[cold]
    fn lock_contended(node: &mut McsNode) {
        while !node.locked.load(Relaxed) {
            hint::spin_loop();
        }
    }
}

pub struct McsLockGuard<'a, 'b, T: ?Sized + 'a> {
    lock: &'a McsLock<T>,
    node: &'b mut McsNode,
    _marker: PhantomData<&'a mut T>,
}

impl<'a, 'b, T: ?Sized> McsLockGuard<'a, 'b, T> {
    fn new(lock: &'a McsLock<T>, node: &'b mut McsNode) -> Self {
        Self {
            lock,
            node,
            _marker: PhantomData,
        }
    }
}

impl<T: ?Sized> Deref for McsLockGuard<'_, '_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `get()` never return a null pointer.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for McsLockGuard<'_, '_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: `get()` never return a null pointer.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for McsLockGuard<'_, '_, T> {
    fn drop(&mut self) {
        // In fact, removing this `load()` will not affect the correctness. On most
        // architectures, however, `compare_exchange()` requires exclusive access to
        // the relevant cacheline regardless of whether the comparison succeeds or
        // not, so manually loading and checking before `compare_exchange()` avoids
        // unnecessarily claming exclusive accesses.
        //
        // See https://marabos.nl/atomics/hardware.html#failing-compare-exchange
        let mut next = self.node.next.load(Relaxed);

        if next.is_null() {
            if self
                .lock
                .tail
                .compare_exchange(self.node, ptr::null_mut(), Release, Relaxed)
                .is_ok()
            {
                return;
            }
            // Fail to reset `tail`, indicating that there is a new waiter here.
            // Loop until `next` pointer being set by the new waiter.
            loop {
                next = self.node.next.load(Relaxed);

                if !next.is_null() {
                    break;
                }
                hint::spin_loop();
            }
        }
        // SAFETY: next is already set at this point.
        unsafe {
            (*next).locked.store(true, Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test1() {
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
    fn test2() {
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
    fn test3() {
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
    fn test4() {
        let sp = McsLock::new(0);

        thread::scope(|s| {
            for _ in 0..10000 {
                s.spawn(|| {
                    let mut node = McsNode::new();
                    *sp.lock(&mut node) += 1
                });
            }
        });

        let mut node = McsNode::new();
        let g = sp.lock(&mut node);
        assert_eq!(*g, 10000);
    }

    #[test]
    fn test5() {
        let sp = McsLock::new(0);

        thread::scope(|s| {
            for i in 0..20000 {
                if i & 1 == 0 {
                    s.spawn(|| {
                        let mut node = McsNode::new();
                        *sp.lock(&mut node) += 1
                    });
                } else {
                    s.spawn(|| {
                        let mut node = McsNode::new();
                        *sp.lock(&mut node) -= 1
                    });
                }
            }
        });

        let mut node = McsNode::new();
        let g = sp.lock(&mut node);
        assert_eq!(*g, 0);
    }
}
