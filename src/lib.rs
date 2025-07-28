//! A simple MCS spin lock implementation

use std::cell::UnsafeCell;
use std::hint;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering::*};

struct SpinLockWaiter {
    pub next: AtomicPtr<SpinLockWaiter>,
    pub locked: AtomicBool,
}

impl SpinLockWaiter {
    fn new() -> Self {
        Self {
            next: AtomicPtr::new(ptr::null_mut()),
            locked: AtomicBool::new(false),
        }
    }
}

pub struct SpinLock<T: ?Sized> {
    tail: AtomicPtr<SpinLockWaiter>,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send + ?Sized> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            tail: AtomicPtr::new(ptr::null_mut()),
            value: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> SpinLock<T> {
    pub fn lock(&self) -> SpinLockGuard<T> {
        let node = Box::into_raw(Box::new(SpinLockWaiter::new()));

        let prev = self.tail.swap(node, Acquire);
        if !prev.is_null() {
            // SAFETY: `prev` is obviously non-null. `node` is also non-null,
            // which is guaranteed by `Box::into_raw()`. Additionally, `prev`
            // will not dangle since the previous waiter would wait until we
            // setup `next` and pass the lock to us before it cleans up.
            unsafe {
                (*prev).next.store(node, Relaxed);
                // spin until we load a true here
                while !(*node).locked.load(Acquire) {
                    hint::spin_loop();
                }
            }
        }
        SpinLockGuard::new(self, node)
    }
}

pub struct SpinLockGuard<'a, T: ?Sized + 'a> {
    lock: &'a self::SpinLock<T>,
    waiter: *mut SpinLockWaiter,
    _phantom: PhantomData<&'a mut T>,
}

impl<'a, T: ?Sized> SpinLockGuard<'a, T> {
    fn new(lock: &'a SpinLock<T>, waiter: *mut SpinLockWaiter) -> Self {
        Self {
            lock,
            waiter,
            _phantom: PhantomData,
        }
    }
}

impl<T: ?Sized> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `get()` never return a null pointer.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: `get()` never return a null pointer.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        if self
            .lock
            .tail
            .compare_exchange(self.waiter, ptr::null_mut(), Release, Relaxed)
            .is_err()
        {
            let mut next;
            // Fail to reset `tail`, indicating that there is a new waiter here.
            // Loop until `next` pointer being set by the new waiter.
            loop {
                // SAFETY: The raw pointer returned by Box::into_raw() is non-null.
                unsafe {
                    next = (*self.waiter).next.load(Relaxed);
                }

                if !next.is_null() {
                    break;
                }
                hint::spin_loop();
            }
            // SAFETY: next is already set at this point.
            unsafe {
                (*next).locked.store(true, Release);
            }
        }
        // SAFETY: raw pointer returned by `Box::into_raw()` is non-null.
        unsafe {
            let _ = Box::from_raw(self.waiter);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test1() {
        let lock = SpinLock::new(0);

        thread::scope(|s| {
            s.spawn(|| *lock.lock() += 1);
            s.spawn(|| {
                let mut g = lock.lock();
                *g += 2;
            });
        });

        let g = lock.lock();
        assert_eq!(*g, 3);
    }

    #[test]
    fn test2() {
        let sp = SpinLock::new(Vec::new());

        thread::scope(|s| {
            s.spawn(|| sp.lock().push(String::from("one")));
            s.spawn(|| {
                let mut g = sp.lock();
                g.push(String::from("two"));
                g.push(String::from("two"));
            });
        });

        let g = sp.lock();
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
        let sp = SpinLock::new(Vec::new());

        thread::scope(|s| {
            s.spawn(|| {
                let mut g = sp.lock();
                g.push("Rust");
                g.push("C");
            });
            s.spawn(|| {
                let mut g = sp.lock();
                g.push("apple");
                g.push("banana");
                g.push("orange");
            });
        });

        let g = sp.lock();
        let result1 = ["Rust", "C", "apple", "banana", "orange"];
        let result2 = ["apple", "banana", "orange", "Rust", "C"];
        assert!(g.as_slice() == &result1 || g.as_slice() == &result2);
    }

    #[test]
    fn test4() {
        let sp = SpinLock::new(0);

        thread::scope(|s| {
            for _ in 0..10000 {
                s.spawn(|| *sp.lock() += 1);
            }
        });

        let g = sp.lock();
        assert_eq!(*g, 10000);
    }

    #[test]
    fn test5() {
        let sp = SpinLock::new(0);

        thread::scope(|s| {
            for i in 0..20000 {
                if i & 1 == 0 {
                    s.spawn(|| *sp.lock() += 1);
                } else {
                    s.spawn(|| *sp.lock() -= 1);
                }
            }
        });

        let g = sp.lock();
        assert_eq!(*g, 0);
    }
}
