use crate::{param::LOGSIZE, spinlock::Mutex};

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

pub static LOG: Mutex<Log> = Mutex::new(Log::new(), "log");

// Contents of the header block, used for both the on-disk header block
// and to keep track in memory of logged block# before commit.
#[repr(C)]
struct LogHeader {
    n: u32,
    block: [u32; LOGSIZE],
}

pub struct Log {
    start: u32,
    size: u32,
    outstanding: u32, // how many FS sys calls are executing.
    committing: bool, // in commit(), please wait.
    dev: u32,
    lh: LogHeader,
}
// dev, start, size は最初の初期化以外は読み込み専用なので、これはむしろSyncOnceCellが向いている
// outstanding, comitting, lh はlockを取って変更するので、mutex必須。
// log は二重な構造体で表現する方がより Rust ぽくて安全といえそうではある。

impl Log {
    const fn new() -> Self {
        Self {
            start: 0,
            size: 0,
            outstanding: 0,
            committing: false,
            dev: 0,
            lh: LogHeader {
                n: 0,
                block: [0; LOGSIZE],
            },
        }
    }
}
