use crate::{condvar::Condvar, spinlock::Mutex};

#[derive(Debug)]
pub struct Semaphore {
    mutex: Mutex<usize>,
    cond: Condvar,
    max: usize,
}

impl Semaphore {
    pub const fn new(max: usize) -> Self {
        Self {
            mutex: Mutex::new(0, "semaphore"),
            cond: Condvar::new(),
            max,
        }
    }

    pub fn wait(&self) {
        let mut cnt = self.mutex.lock();
        while *cnt >= self.max {
            cnt = self.cond.wait(cnt);
        }
        *cnt += 1;
    }

    pub fn post(&self) {
        let mut cnt = self.mutex.lock();
        assert!(*cnt > 0);
        *cnt -= 1;
        if *cnt <= self.max {
            self.cond.notify_all();
        }
    }
}
