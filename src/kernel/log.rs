use core::ops::{Deref, DerefMut};
use crate::{
    lazy::SyncLazy,
    param::{LOGSIZE, ROOTDEV, MAXOPBLOCKS},
    spinlock::Mutex,
    fs::{SB, BSIZE}, bio::{BCACHE, BufGuard}, proc::{CPUS, Process, PROCS},
};

// Simple logging that allows concurrent FS system calls.
//
// A log transaction contains the updates of multiple FS system
// calls. The logging system only commits when there are
// no FS system calls active. Thus there is never
// any reasoning required about whether a commit might
// write an uncommitted system call's updates to disk.
//
// A system call should call begin_op()/end_op() to mark
// its start and end. Usually begin_op() just increments
// the count of in-progress FS system calls and returns.
// But if it thinks the log is close to running out, it
// sleeps until the last outstanding end_op() commits.
//
// The log is a physical re-do log containing disk blocks.
// The on-disk log format:
//   header block, containing block #s for block A, B, C, ...
//   block A
//   block B
//   block C
//   ...
// Log appends are synchronous.

pub static LOG: SyncLazy<Mutex<Log>> = SyncLazy::new(|| Mutex::new(Log::new(ROOTDEV), "log"));

// Contents of the header block, used for both the on-disk header block
// and to keep track in memory of logged block# before commit.
#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
struct LogHeader {
    n: u32,
    block: [u32; LOGSIZE],
}

pub struct Log {
    start: u32,
    size: u32,
    dev: u32,
    outstanding: u32,
    committing: bool,
    lh: LogHeader,
}

impl Log {
    fn new(dev: u32) -> Self {
        let sb = SB.get().unwrap();
        let mut log =  Self {
            start: sb.logstart,
            size: sb.nlog,
            dev,
            outstanding: 0,
            committing: false,
            lh: LogHeader { n: 0, block: [0; LOGSIZE] },
        };
        log.recover();
        log
    }

    fn recover(&mut self) {
        self.read_head();
        self.install_trans(true); // if committed, copy from log to disk
        self.lh.n = 0;
        self.write_head(); // clear the log
    } 

    // Read the log header from disk into in-memory log header
    fn read_head(&mut self) {
        let buf = BCACHE.read(self.dev, self.start);
        let lh = buf.align_to::<LogHeader>().get(0).unwrap();
        self.lh = *lh;
    }

    
    // Copy comitted blocks from log to their home location
    fn install_trans(&self, recovering: bool) {
        for tail in 0..self.lh.n {
            let lbuf = BCACHE.read(self.dev, self.start + tail + 1); // read log block
            let mut dbuf = BCACHE.read(self.dev, self.lh.block[tail as usize]); // read dst
            dbuf.copy_from_slice(lbuf.deref().deref()); // copy block to dst
            dbuf.write(); // write dst to disk
            if !recovering {
                unsafe { dbuf.unpin() };
            }
        }
    }

    // Write in-memory log header to disk.
    // This is the true point at which the
    // current transaction commits.
    fn write_head(&self) {
        let mut buf = BCACHE.read(self.dev, self.start);
        let hb = buf.align_to_mut::<LogHeader>().get_mut(0).unwrap();
        *hb = self.lh;
        buf.write();
    }

    // Copy modified blocks from cache to log.
    fn write_log(&mut self) {
        for tail in 0..self.lh.n {
            let mut to = BCACHE.read(self.dev, self.start + tail + 1); // log block
            let from = BCACHE.read(self.dev, self.lh.block[tail as usize]); // cache block
            to.copy_from_slice(from.deref().deref());
            to.write(); // write the log
        }
    }

    fn commit(&mut self) {
        if self.lh.n > 0 {
            self.write_log(); // Write modified blocks from cache to log
            self.write_head(); // Wrtie header to disk -- the real commit
            self.install_trans(false); // Now install writes to home locations
            self.lh.n = 0;
            self.write_head();
        }
    }
}

impl Mutex<Log> {
    pub fn init(&self) {
        // SyncLazy initialization
        assert!(core::mem::size_of::<LogHeader>() >= BSIZE, "initlog: too big log header");
    }
    // called at the start of each FS system call.
    pub fn begin_op(&self) {
        let mut guard = self.lock();
        let p = CPUS.my_proc().unwrap();
        loop {
            if guard.committing {
                guard = p.sleep(guard.deref() as *const _ as usize, guard);
            } else if (guard.lh.n as usize + (guard.outstanding + 1) as usize * MAXOPBLOCKS) > LOGSIZE {
                // this op might exhaust log space; wait for commit.
                guard = p.sleep(guard.deref() as *const _ as usize, guard);
            } else {
                guard.outstanding += 1;
                break;
            }
        }
    }

    // called at the end of each FS system call.
    // commits if this was the last outstanding operation.
    pub fn end_op(&self) {
        let mut log: Option<*mut Log> = None;

        {
            let mut guard = self.lock();
            guard.outstanding -= 1;
            if guard.committing {
                panic!("log.commiting");
            }
            if guard.outstanding == 0 {
                log.replace(guard.deref_mut() as *mut _);
                guard.committing = true;
            } else {
                // begin_op() may be waiting for log space,
                // and decrementing log.outstanding has decreased
                // the amount of reserved space.
                PROCS.wakeup(guard.deref() as *const _ as usize);
            }
        }

        if let Some(log) = log {
            // call commit w/o holding locks, since not allowed
            // to sleep with locks.
            unsafe {
                log.as_mut().unwrap().commit();
            }
            let mut guard = self.lock();
            guard.committing = false;
            PROCS.wakeup(guard.deref() as *const _ as usize);
        }
    }

    // Caller has modified b->data and is done with the buffer.
    // Record the block number and pin in the cache by increasing refcnt.
    // commit()/write() will do the disk write.
    // 
    // LOG.write() replaces BudGuard.write(); a typical use is:
    // bp = BCACHE.read();
    // modify bp.data[]
    // LOG.write(bp)
    pub fn write(&self, b: BufGuard) {
        let mut guard = self.lock();
        if guard.lh.n as usize >= LOGSIZE || guard.lh.n >= guard.size - 1 {
            panic!("too big a transaction");
        }
        if guard.outstanding < 1 {
            panic!("LOG.write outside of trans");
        }

        let blockno = b.buf().ctrl.read().blockno;
        for i in 0..guard.lh.n {
            if guard.lh.block[i as usize] == blockno { // log absorption
                return;
            }
        }
        let n = guard.lh.n as usize;
        guard.lh.block[n] = blockno;
        unsafe { b.pin() };
        guard.lh.n += 1;
    }
}
