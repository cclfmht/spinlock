//! A simple ticket spin lock implementation

use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU16, Ordering::*};

pub struct SpinLock<T: ?Sized> {
    next: AtomicU16,
    owner: AtomicU16,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send + ?Sized> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            next: AtomicU16::new(0),
            owner: AtomicU16::new(0),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<T> {
        let num = self.next.fetch_add(1, AcqRel);

        while num != self.owner.load(Acquire) {
            std::hint::spin_loop();
        }

        SpinLockGuard {
            lock: self,
            _phantom: PhantomData,
        }
    }
}

pub struct SpinLockGuard<'a, T: ?Sized + 'a> {
    lock: &'a self::SpinLock<T>,
    _phantom: PhantomData<&'a mut T>,
}

impl<T: ?Sized> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: get() never return a null pointer.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: get() never return a null pointer.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.owner.fetch_add(1, Release);
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
        let sp = SpinLock::new(0);

        thread::scope(|s| {
            for _ in 0..1000 {
                s.spawn(|| *sp.lock() += 1);
            }
        });

        let g = sp.lock();
        assert_eq!(*g, 1000);
    }

    #[test]
    fn test3() {
        let sp = SpinLock::new(0);

        thread::scope(|s| {
            for i in 0..2000 {
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

    #[test]
    fn test4() {
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
    fn test5() {
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
}
