// Console input and output, to the uart.
// Reads are line at a time.
// Implements special input characters:
//   newline -- end of line
//   control-h -- backspace
//   control-u -- kill line
//   control-d -- end of line
//   control-p -- print process list

use crate::defs::as_bytes;
use crate::file::{Device, DEVSW, Major};
use crate::proc::{procdump, CopyInOut, Process, CPUS, PROCS};
use crate::spinlock::Mutex;
use crate::uart;
use crate::vm::UVAddr;
use core::num::Wrapping;

pub static CONS: Mutex<Cons> = Mutex::new(Cons::new(), "cons");

const BS: u8 = 0x08;

// Control-x
const fn ctrl(x: u8) -> u8 {
    x - b'@'
}

const INPUT_BUF: usize = 128;
pub struct Cons {
    buf: [u8; INPUT_BUF],
    r: Wrapping<usize>, // Read index
    w: Wrapping<usize>, // Write index
    e: Wrapping<usize>, // Edit index
}

impl Cons {
    const fn new() -> Cons {
        Cons {
            buf: [0; INPUT_BUF],
            r: Wrapping(0),
            w: Wrapping(0),
            e: Wrapping(0),
        }
    }
}
impl Device<UVAddr> for Mutex<Cons> {
    //
    // user read()s from the console go here.
    // copy (up to) a whole input line to dst.
    //
    fn read(&self, dst: &mut [u8]) -> Result<usize, ()> {
        let mut cons_guard = self.lock();
        let mut c;
        let p = CPUS.my_proc().unwrap();
        let mut size = 0;
        for (n, cdst) in (1..).zip(dst.iter_mut()) {
            // wait until interrupt handler has put some
            // input into CONS.buf.
            while cons_guard.r == cons_guard.w {
                if p.inner.lock().killed {
                    return Err(());
                }
                cons_guard = p.sleep(&cons_guard.r as *const _ as usize, cons_guard);
            }
            c = cons_guard.buf[cons_guard.r.0 % INPUT_BUF];
            cons_guard.r += Wrapping(1);

            if c == ctrl(b'D') {
                // end of line
                if n > 0 {
                    // Save ^D for next time, to make sure
                    // caller gets a 0-bytes result.
                    cons_guard.r -= Wrapping(1);
                }
                break;
            }
            // copy the input byte to the user-space buffer.
            if unsafe {
                p.either_copyout(From::from(Self::to_va(as_bytes(cdst))), &c)
                    .is_err()
            } {
                break;
            }
            size = n;

            if c == b'\n' {
                // a whole line has arrived, return to
                // the user-level read().
                break;
            }
        }
        Ok(size)
    }

    //
    // user write()s to the console go here.
    //
    fn write(&self, src: &[u8]) -> Result<usize, ()> {
        let mut c = 0;
        for (n, csrc) in src.iter().enumerate() {
            let p = CPUS.my_proc().unwrap();
            if unsafe {
                p.either_copyin(&mut c, From::from(Self::to_va(as_bytes(csrc))))
                    .is_err()
            } {
                return Ok(n);
            }
            putc(c);
        }
        Ok(src.len())
    }

    fn major(&self) -> Major {
        Major::Console
    }
}

impl Mutex<Cons> {
    //
    // the console input interrupt handler.
    // CONS.intr() calls this for input character.
    // do erase/kill processing, append to cons.buf,
    // wake up CONS.read() if a whole line has arrived.
    //
    pub fn intr(&self, c: u8) {
        let mut cons_guard = self.lock();
        match c {
            // Print process list
            m if m == ctrl(b'P') => procdump(),
            // Kill line
            m if m == ctrl(b'U') => {
                while cons_guard.e != cons_guard.w
                    && cons_guard.buf[(cons_guard.e - Wrapping(1)).0 % INPUT_BUF] != b'\n'
                {
                    cons_guard.e -= Wrapping(1);
                    putc(ctrl(b'H'));
                }
            }
            // Backspace
            m if m == ctrl(b'H') | b'\x7f' => {
                if cons_guard.e != cons_guard.w {
                    cons_guard.e -= Wrapping(1);
                    putc(ctrl(b'H'));
                }
            }
            _ => {
                if c != 0 && (cons_guard.e - cons_guard.r).0 < INPUT_BUF {
                    let c = if c == b'\r' { b'\n' } else { c };

                    // echo back to the user
                    putc(c);

                    // store for consumption by CONS.read().
                    let e_idx = cons_guard.e.0 % INPUT_BUF;
                    cons_guard.buf[e_idx] = c;
                    cons_guard.e += Wrapping(1);

                    if c == b'\n' || c == ctrl(b'D') || (cons_guard.e - cons_guard.r).0 == INPUT_BUF
                    {
                        // wake up CONS.read() if a whole line (or end of line)
                        // has arrived
                        cons_guard.w = cons_guard.e;
                        PROCS.wakeup(&cons_guard.r as *const _ as usize);
                    }
                }
            }
        }
    }
}

pub fn init() {
    unsafe { uart::init() }
    DEVSW.set(Major::Console, &CONS).unwrap();
}

//
// send one character to the uart.
// called by printf, and to echo input characters,
// but not from write().
//
pub fn putc(c: u8) {
    if c == ctrl(b'H') {
        uart::putc_sync(BS);
        uart::putc_sync(b' ');
        uart::putc_sync(BS);
    } else {
        uart::putc_sync(c);
    }
}
