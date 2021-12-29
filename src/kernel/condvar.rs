use crate::{
    proc::{Process, CPUS, PROCS},
    spinlock::MutexGuard,
};

pub struct Condvar;

impl Condvar {
    pub const fn new() -> Self {
        Self
    }
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let p = CPUS.my_proc().unwrap();
        p.sleep(self as *const _ as usize, guard)
    }
    pub fn notify_all(&self) {
        PROCS.wakeup(self as *const _ as usize);
    }
}
