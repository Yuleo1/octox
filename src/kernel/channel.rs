use crate::condvar::Condvar;
use crate::semaphore::Semaphore;
use crate::spinlock::Mutex;
use alloc::{collections::LinkedList, sync::Arc};

#[derive(Clone)]
pub struct SyncSender<T> {
    sem: Arc<Semaphore>,
    buf: Arc<Mutex<LinkedList<T>>>,
    cond: Arc<Condvar>,
}

impl<T: Send> SyncSender<T> {
    pub fn send(&self, data: T) {
        self.sem.wait();
        let mut buf = self.buf.lock();
        buf.push_back(data);
        self.cond.notify_all();
    }
}

pub struct Receiver<T> {
    sem: Arc<Semaphore>,
    buf: Arc<Mutex<LinkedList<T>>>,
    cond: Arc<Condvar>,
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> T {
        let mut buf = self.buf.lock();
        loop {
            if let Some(data) = buf.pop_front() {
                self.sem.post();
                break data;
            }
            buf = self.cond.wait(buf);
        }
    }
}

pub fn sync_channel<T>(max: usize) -> (SyncSender<T>, Receiver<T>) {
    let sem = Arc::new(Semaphore::new(max));
    let buf = Arc::new(Mutex::new(LinkedList::new(), "sync channel"));
    let cond = Arc::new(Condvar::new());
    let tx = SyncSender {
        sem: Arc::clone(&sem),
        buf: Arc::clone(&buf),
        cond: Arc::clone(&cond),
    };
    let rx = Receiver { sem, buf, cond };
    (tx, rx)
}
