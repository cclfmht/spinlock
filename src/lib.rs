//! A simple MCS spin lock implementation

#![no_std]

use core::cell::UnsafeCell;
use core::hint;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering::*, fence};

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
    pub fn try_lock<'lock, 'node>(
        &'lock self,
        node: &'node mut McsNode,
    ) -> Option<McsLockGuard<'lock, 'node, T>> {
        Self::init_node(node);

        if self
            .tail
            .compare_exchange(ptr::null_mut(), node, AcqRel, Relaxed)
            .is_ok()
        {
            Some(McsLockGuard::new(self, node))
        } else {
            None
        }
    }

    pub fn lock<'lock, 'node>(
        &'lock self,
        node: &'node mut McsNode,
    ) -> McsLockGuard<'lock, 'node, T> {
        Self::init_node(node);

        // If thread A sets `tail` with its node and then thread B loads that value, then the
        // initialization of A's node should be visible to B at this point so that we can safely
        // set `next` on that node later. Thus, we need to establish a happens-before relationship
        // between the store in A and the load in B.
        let prev = self.tail.swap(node, AcqRel);

        if !prev.is_null() {
            // SAFETY: `prev` is obviously non-null.
            unsafe {
                // When the previous node loads `next` set by us on its node, the initialization of
                // our node should be visible to it so that it can safely set `locked` on our node.
                (*prev).next.store(node, Release);
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

    fn init_node(node: &mut McsNode) {
        *node.next.get_mut() = ptr::null_mut();
        *node.locked.get_mut() = false;
    }

    #[cold]
    fn lock_contended(node: &mut McsNode) {
        while !node.locked.load(Relaxed) {
            hint::spin_loop();
        }
    }
}

pub struct McsLockGuard<'lock, 'node, T: ?Sized + 'lock> {
    lock: &'lock McsLock<T>,
    node: &'node mut McsNode,
    _marker: PhantomData<&'lock mut T>,
}

impl<'lock, 'node, T: ?Sized> McsLockGuard<'lock, 'node, T> {
    fn new(lock: &'lock McsLock<T>, node: &'node mut McsNode) -> Self {
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
        // Synchronize with the Release store on `next` by the next waiter
        fence(Acquire);
        // SAFETY: next is already set at this point.
        unsafe {
            (*next).locked.store(true, Release);
        }
    }
}
