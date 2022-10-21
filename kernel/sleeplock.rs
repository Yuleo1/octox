use crate::{
    proc::{Process, CPUS, PROCS},
    spinlock::Mutex,
};
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

// Sleeping locks

// Long-term locks for processes
#[derive(Debug)]
pub struct SleepLock<T> {
    lk: Mutex<SleepLockInfo>, // spinlock protecting this sleep lock
    data: UnsafeCell<T>,
    name: &'static str, // Name of lock
}
unsafe impl<T> Sync for SleepLock<T> {}
unsafe impl<T> Send for SleepLock<T> {}

#[derive(Debug)]
struct SleepLockInfo {
    locked: bool,
    pid: usize,
}

pub struct SleepLockGuard<'a, T> {
    sleep_lock: &'a SleepLock<T>,
}

impl SleepLockInfo {
    pub const fn new(locked: bool, pid: usize) -> Self {
        SleepLockInfo { locked, pid }
    }
}

impl<T> SleepLock<T> {
    pub const fn new(data: T, name: &'static str) -> Self {
        Self {
            lk: Mutex::new(SleepLockInfo::new(false, 0), "sleep lock"),
            data: UnsafeCell::new(data),
            name,
        }
    }

    pub fn lock(&self) -> SleepLockGuard<'_, T> {
        let mut lk = self.lk.lock();
        let p = CPUS.my_proc().unwrap();
        while lk.locked {
            lk = p.sleep(self as *const _ as usize, lk);
        }
        lk.locked = true;
        lk.pid = p.pid();
        SleepLockGuard { sleep_lock: &self }
    }

    pub fn holding(&self) -> bool {
        let lk = self.lk.lock();
        lk.locked && lk.pid == CPUS.my_proc().unwrap().pid()
    }

    pub fn unlock(guard: SleepLockGuard<'_, T>) -> &'_ SleepLock<T> {
        guard.sleep_lock()
    }
}

impl<'a, T: 'a> SleepLockGuard<'a, T> {
    // Returns a reference to the original 'Mutex' object.
    pub fn sleep_lock(&self) -> &'a SleepLock<T> {
        self.sleep_lock
    }

    pub fn holding(&self) -> bool {
        self.sleep_lock.holding()
    }
}
impl<'a, T: 'a> Deref for SleepLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.sleep_lock.data.get() }
    }
}

impl<'a, T: 'a> DerefMut for SleepLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.sleep_lock.data.get() }
    }
}

impl<'a, T: 'a> Drop for SleepLockGuard<'a, T> {
    fn drop(&mut self) {
        assert!(
            self.sleep_lock.holding(),
            "release {}",
            self.sleep_lock.name
        );
        let mut lk = self.sleep_lock.lk.lock();
        lk.locked = false;
        lk.pid = 0;
        PROCS.wakeup(self.sleep_lock as *const _ as usize);
    }
}
