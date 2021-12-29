use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::proc::{IntrLock, CPUS};

pub struct RwLock<T: ?Sized> {
    rcnt: AtomicUsize,
    wcnt: AtomicBool,
    data: UnsafeCell<T>,
}
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}
pub struct RwLockReadGuard<'a, T: ?Sized + 'a> {
    lock: &'a RwLock<T>,
    _intr_lock: IntrLock<'a>,
}
impl<T: ?Sized> !Send for RwLockReadGuard<'_, T> {}
unsafe impl<T: ?Sized + Sync> Sync for RwLockReadGuard<'_, T> {}

pub struct RwLockWriteGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
    _intr_lock: IntrLock<'a>,
}
impl<T: ?Sized> !Send for RwLockWriteGuard<'_, T> {}
unsafe impl<T: ?Sized + Sync> Sync for RwLockWriteGuard<'_, T> {}

impl<T> RwLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            rcnt: AtomicUsize::new(0),
            wcnt: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> RwLock<T> {
    pub fn get_mut(&mut self) -> &mut T {
        // We know statically that there are no other reference to `self`, so
        // there's no need to lock the inner lock.
        self.data.get_mut()
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        loop {
            match self.try_read() {
                Some(guard) => break guard,
                None => core::hint::spin_loop(),
            }
        }
    }

    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        let _intr_lock = CPUS.intr_lock();
        self.rcnt.fetch_add(1, Ordering::Acquire);
        if self.wcnt.load(Ordering::Relaxed) {
            self.rcnt.fetch_sub(1, Ordering::Release);
            None
        } else {
            Some(RwLockReadGuard {
                lock: self,
                _intr_lock,
            })
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        loop {
            match self.try_write() {
                Ok(guard) => break guard,
                Err(_) => core::hint::spin_loop(),
            }
        }
    }

    pub fn try_write(&self) -> Result<RwLockWriteGuard<'_, T>, ()> {
        let _intr_lock = CPUS.intr_lock();
        if self
            .wcnt
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
            && self.rcnt.load(Ordering::Relaxed) < 1
        {
            Ok(RwLockWriteGuard {
                lock: self,
                _intr_lock,
            })
        } else {
            Err(())
        }
    }
}

impl<'a, T: ?Sized> RwLockReadGuard<'a, T> {
    pub fn upgrade(mut self) -> RwLockWriteGuard<'a, T> {
        loop {
            self = match self.try_upgrade() {
                Ok(guard) => break guard,
                Err(e) => e,
            };
            core::hint::spin_loop();
        }
    }
    fn try_upgrade(self) -> Result<RwLockWriteGuard<'a, T>, Self> {
        let _intr_lock = CPUS.intr_lock();
        if self
            .lock
            .wcnt
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
            && self.lock.rcnt.load(Ordering::Relaxed) == 1
        {
            let lock = self.lock;
            Ok(RwLockWriteGuard { lock, _intr_lock })
        } else {
            Err(self)
        }
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        assert!(self.lock.rcnt.load(Ordering::Relaxed) > 0);
        self.lock.rcnt.fetch_sub(1, Ordering::Release);
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        assert!(self.lock.wcnt.load(Ordering::Relaxed));
        self.lock.wcnt.store(false, Ordering::Release);
    }
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}
