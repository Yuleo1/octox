use core::mem::size_of;

use crate::{proc::{Proc, CPUS, ProcData, Trapframe}, vm::UVAddr};
use alloc::sync::Arc;

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

pub struct SysCalls<'a> {
    proc: &'a Arc<Proc>,
    data: &'a mut ProcData,
    tf: &'a mut Trapframe,
}

impl SysCalls<'_> {
    pub fn arg(&self, n: usize) -> usize {
        match n {
            0 => self.tf.a0,
            1 => self.tf.a1,
            2 => self.tf.a2,
            3 => self.tf.a3,
            4 => self.tf.a4,
            5 => self.tf.a5,
            _ => panic!("arg"),
        }
    }

    // pub fn arg_addr(&self, arg: usize) -> UVAddr {
    //     UVAddr::from(arg)
    // }


    // // Fetch the data at addr from the current process.
    // pub fn fetch_addr(&self, addr: UVAddr, buf: &mut [u8]) -> Result<(), ()> {
    //     if addr >= self.data.sz || addr + size_of::<usize>() > self.data.sz { // both tests needed, in case of overflow
    //         return Err(())
    //     }

    //     self.data.uvm.unwrap().copyin(buf, addr)
    // }


    fn call(&mut self) -> () {
        if let Ok(res) = match SysCallNum::from_usize(self.tf.a7) {
            Some(SysCallNum::SysFork) => todo!(),
            Some(SysCallNum::SysExit) => todo!(),
            Some(SysCallNum::SysWait) => todo!(),
            Some(SysCallNum::SysPipe) => todo!(),
            Some(SysCallNum::SysRead) => todo!(),
            Some(SysCallNum::SysKill) => todo!(),
            Some(SysCallNum::SysExec) => todo!(),
            Some(SysCallNum::SysFstat) => todo!(),
            Some(SysCallNum::SysChdir) => todo!(),
            Some(SysCallNum::SysDup) => todo!(),
            Some(SysCallNum::SysGetpid) => todo!(),
            Some(SysCallNum::SysSbrk) => todo!(),
            Some(SysCallNum::SysSleep) => todo!(),
            Some(SysCallNum::SysUptime) => todo!(),
            Some(SysCallNum::SysOpen) => todo!(),
            Some(SysCallNum::SysWrite) => todo!(),
            Some(SysCallNum::SysMknod) => todo!(),
            Some(SysCallNum::SysUnlink) => todo!(),
            Some(SysCallNum::SysLink) => todo!(),
            Some(SysCallNum::SysMkdir) => todo!(),
            Some(SysCallNum::SysClose) => todo!(),
            None => {
                println!("unknown sys call {}", self.tf.a7);
                Err(())
            }
        } {
            self.tf.a0 = res;
        } else {
            self.tf.a0 = -1 as isize as usize;
        }
    }

}

pub fn syscall() {
    let p = CPUS.my_proc().unwrap();
    let data: &mut ProcData; 
    let tf: &mut Trapframe;
    unsafe {
        data = &mut *p.data.get();
        tf =  data.trapframe.unwrap().as_mut();
    }
    
    let mut syscalls = SysCalls { proc: p, data, tf };
    syscalls.call()
}
