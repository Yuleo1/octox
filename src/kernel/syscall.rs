use alloc::sync::Arc;

use crate::proc::{Proc, CPUS};

// System call numbers
#[repr(usize)]
pub enum SysCallNum {
    SysFork = 1,
    SysExit = 2,
    SysWait = 3,
    SysPipe = 4,
    SysRead = 5,
    SysKill = 6,
    SysExec = 7,
    SysFstat = 8,
    SysChdir = 9,
    SysDup = 10,
    SysGetpid = 11,
    SysSbrk = 12,
    SysSleep = 13,
    SysUptime = 14,
    SysOpen = 15,
    SysWrite = 16,
    SysMknod = 17,
    SysUnlink = 18,
    SysLink = 19,
    SysMkdir = 20,
    SysClose = 21,
}

impl SysCallNum {
    fn from_usize(n: usize) -> Option<Self> {
        match n {
            1 => Some(Self::SysFork),
            2 => Some(Self::SysExit),
            3 => Some(Self::SysWait),
            4 => Some(Self::SysPipe),
            5 => Some(Self::SysRead),
            6 => Some(Self::SysKill),
            7 => Some(Self::SysExec),
            8 => Some(Self::SysFstat),
            9 => Some(Self::SysChdir),
            10 => Some(Self::SysDup),
            11 => Some(Self::SysGetpid),
            12 => Some(Self::SysSbrk),
            13 => Some(Self::SysSleep),
            14 => Some(Self::SysUptime),
            15 => Some(Self::SysOpen),
            16 => Some(Self::SysWrite),
            17 => Some(Self::SysMknod),
            18 => Some(Self::SysUnlink),
            19 => Some(Self::SysLink),
            20 => Some(Self::SysMkdir),
            21 => Some(Self::SysClose),
            _ => None,
        }
    }
}

type SysResult = Result<usize, ()>;
static SYSCALLS: [fn() -> SysResult; 1] = [hogehoge];

pub fn hogehoge() -> SysResult {
    Err(())
}

pub struct SysCalls<'a> {
    proc: &'a Arc<Proc>,
}

impl SysCalls<'_> {
    fn call(&self) -> () {
        let data = unsafe { &mut *self.proc.data.get() };
        let mut tf = unsafe { data.trapframe.unwrap().as_mut() };
        if let Ok(res) = match SysCallNum::from_usize(tf.a7) {
            Some(num) => SYSCALLS[num as usize](),
            None => {
                println!("unknown sys call {}", tf.a7);
                Err(())
            }
        } {
            tf.a0 = res;
        } else {
            tf.a0 = -1 as isize as usize;
        }
    }
}

pub fn syscall() {
    let p = CPUS.my_proc().unwrap();
    let syscalls = SysCalls { proc: p };
    syscalls.call()
}
