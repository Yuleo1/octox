// Buffer cache.
//
// The buffer cache is a linked list of buf structures holding
// cached copies of disk block contents. Caching disk blocks
// in memory reduces the number of disk blocks used by multiple processes.
//
// Interface:
// * To get a buffer for a particular disk block, clla bread.
// * After changing buffer data, call bwrite to write it to disk.
// * When done with the buffer, call brelse.
// * Do not use the buffer after calling brelse.
// * Only one process at a time can use a buffer,
//     so do not keep them longer than necessary.

use crate::{
    fs::BSIZE,
    param::NBUF,
    rwlock::RwLock,
    sleeplock::{SleepLock, SleepLockGuard},
    spinlock::Mutex,
    virtio_disk::DISK,
};
use alloc::sync::{Arc, Weak};
use array_macro::array;
use core::ops::{Deref, DerefMut};

pub static BCACHE: BCache = BCache::new();

pub struct Buf {
    data: &'static SleepLock<[u8; BSIZE]>,
    pub ctrl: RwLock<Ctrl>,
}

pub struct Ctrl {
    valid: bool,    // has data been read from disk?
    pub disk: bool, // does disk "own" buf?
    dev: u32,
    pub blockno: u32,
    next: Option<Arc<Buf>>,
    prev: Option<Weak<Buf>>,
}

pub struct BufGuard<'a> {
    sleeplock: Option<SleepLockGuard<'static, [u8; BSIZE]>>,
    buf: Option<Arc<Buf>>,
    bcache: &'a BCache,
}

pub struct BCache {
    buf: [SleepLock<[u8; BSIZE]>; NBUF],
    // Linked list of all buffers, sorted by how recently the
    // buffer was used.
    lru: Mutex<Lru>,
}

struct Lru {
    head: Option<Arc<Buf>>,
    tail: Option<Weak<Buf>>,
    n: usize,
}

impl Lru {
    const fn new() -> Self {
        Self {
            head: None,
            tail: None,
            n: 0,
        }
    }

    fn len(&self) -> usize {
        self.n
    }

    fn add(&mut self, buf: Arc<Buf>) {
        match self.tail.take() {
            Some(old_tail) => {
                old_tail.upgrade().unwrap().ctrl.write().next = Some(Arc::clone(&buf));
                buf.ctrl.write().prev = Some(old_tail);
            }
            None => {
                self.head = Some(Arc::clone(&buf));
            }
        }
        self.tail = Some(Arc::downgrade(&buf));
        self.n += 1;
    }

    fn relse(&mut self, mut buf: Arc<Buf>) {
        assert!(Arc::strong_count(&buf) > 1);
        unsafe {
            let ptr = Arc::into_raw(buf);
            Arc::decrement_strong_count(ptr);
            buf = Arc::from_raw(ptr);
        }
        if Arc::strong_count(&buf) == 1 {
            let next = buf.ctrl.write().next.take();
            let prev = buf.ctrl.write().prev.take();
            if let Some(n) = next.as_ref() {
                n.ctrl.write().prev = prev.clone();
            }
            if let Some(p) = prev.as_ref() {
                p.upgrade().unwrap().ctrl.write().next = next.clone();
            }
            match self.head.take() {
                Some(old_head) => {
                    old_head.ctrl.write().prev = Some(Arc::downgrade(&buf));
                    buf.ctrl.write().next = Some(old_head);
                }
                None => {
                    self.tail = Some(Arc::downgrade(&buf));
                }
            }
            self.head = Some(buf);
        }
    }

    fn iter<'a>(&'a self) -> Iter<'a> {
        Iter {
            head: self.head.as_ref(),
            tail: self.tail.as_ref().and_then(|prev| unsafe {
                // Need an unbound lifetime to get 'a
                prev.upgrade().as_ref().map(|prev| &*(prev as *const _))
            }),
            len: self.len(),
        }
    }
}

pub struct Iter<'a> {
    head: Option<&'a Arc<Buf>>,
    tail: Option<&'a Arc<Buf>>,
    len: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Arc<Buf>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            self.head.map(|n| unsafe {
                self.len -= 1;
                // Need an unbound lifetime to get 'a
                self.head = n.ctrl.read().next.as_ref().map(|next| &*(next as *const _));
                n
            })
        }
    }
}

impl<'a> DoubleEndedIterator for Iter<'a> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            None
        } else {
            self.tail.map(|n| unsafe {
                self.len -= 1;
                // Need an unbound lifetime to get 'a
                self.tail = n
                    .ctrl
                    .read()
                    .prev
                    .as_ref()
                    .and_then(|prev| prev.upgrade().as_ref().map(|prev| &*(prev as *const _)));
                n
            })
        }
    }
}

impl BCache {
    const fn new() -> Self {
        Self {
            buf: array![_ => SleepLock::new([0; BSIZE], "buffer"); NBUF],
            lru: Mutex::new(Lru::new(), "bcache"),
        }
    }

    fn get(&self, dev: u32, blockno: u32) -> BufGuard {
        let lru = self.lru.lock();
        // Is the block already cached?
        for b in lru.iter() {
            let ctrl = b.ctrl.read();
            if ctrl.dev == dev && ctrl.blockno == blockno {
                return BufGuard {
                    sleeplock: Some(b.data.lock()),
                    buf: Some(Arc::clone(b)),
                    bcache: self,
                };
            }
        }

        // Not cached.
        // Recycle the least recentrly used (LRU) unused buffer
        for b in lru.iter().rev() {
            println!("bcount: {}", Arc::strong_count(b));
            if Arc::strong_count(b) == 1 {
                let mut ctrl = b.ctrl.write();
                ctrl.dev = dev;
                ctrl.blockno = blockno;
                ctrl.valid = false;
                return BufGuard {
                    sleeplock: Some(b.data.lock()),
                    buf: Some(Arc::clone(b)),
                    bcache: self,
                };
            }
        }
        panic!("no buffers");
    }

    // Return a locked buf with the contents of the indicated block.
    pub fn read(&self, dev: u32, blockno: u32) -> BufGuard {
        let b = self.get(dev, blockno);
        if !b.buf.as_ref().unwrap().ctrl.read().valid {
            DISK.rw(b.buf(), b.raw_data(), false);
            b.buf.as_ref().unwrap().ctrl.write().valid = true;
        }
        b
    }
}

impl<'a> BufGuard<'a> {
    // Write buf's content to disk. Must be locked.
    pub fn write(&self) {
        if !self.holding() {
            panic!("bwrite");
        }
        DISK.rw(self.buf(), self.raw_data(), true);
    }

    pub fn raw_data(&self) -> *const [u8; BSIZE] {
        self.deref().deref()
    }

    pub fn buf(&self) -> &'static Arc<Buf> {
        unsafe { &*(self.buf.as_ref().unwrap() as *const _) }
    }

    pub unsafe fn pin(&self) {
        Arc::decrement_strong_count(Arc::as_ptr(self.buf.as_ref().unwrap()));
    }

    pub unsafe fn unpin(&self) {
        Arc::increment_strong_count(Arc::as_ptr(self.buf.as_ref().unwrap()))
    }

    pub fn align_to<U>(&self) -> &[U] {
        let (head, body, _) = unsafe { self.sleeplock.as_ref().unwrap().align_to::<U>() };
        assert!(head.is_ascii(), "Data was not aligned");
        body
    }
    pub fn align_to_mut<U>(&mut self) -> &mut [U] {
        let (head, body, _) = unsafe { self.sleeplock.as_mut().unwrap().align_to_mut::<U>() };
        assert!(head.is_ascii(), "Data was not aligned");
        body
    }
}

impl<'a> Deref for BufGuard<'a> {
    type Target = SleepLockGuard<'static, [u8; BSIZE]>;
    fn deref(&self) -> &Self::Target {
        self.sleeplock.as_ref().unwrap()
    }
}

impl<'a> DerefMut for BufGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.sleeplock.as_mut().unwrap()
    }
}

impl<'a> Drop for BufGuard<'a> {
    fn drop(&mut self) {
        if !self.holding() {
            panic!("drop - brelse");
        }
        SleepLock::unlock(self.sleeplock.take().unwrap());
        let mut lru = self.bcache.lru.lock();
        lru.relse(self.buf.take().unwrap());
    }
}

impl Buf {
    const fn new(buf: &'static SleepLock<[u8; BSIZE]>) -> Self {
        Self {
            data: buf,
            ctrl: RwLock::new(Ctrl {
                valid: false,
                disk: false,
                dev: 0,
                blockno: 0,
                next: None,
                prev: None,
            }),
        }
    }
}

pub fn init() {
    let lru = unsafe { BCACHE.lru.get_mut() };
    // Create linked list of buffers
    for b in BCACHE.buf.iter() {
        lru.add(Arc::new(Buf::new(b)));
    }
}
