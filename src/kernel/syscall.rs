use crate::{
    proc::{Proc, ProcData, Trapframe, CPUS},
    sysctbl::SysCallNum,
    vm::{Addr, UVAddr},
};
use alloc::sync::Arc;
use core::mem::size_of_val;

pub struct SysCalls<'a> {
    pub proc: &'a Arc<Proc>,
    pub data: &'a mut ProcData,
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

    // Retrieve an argument as a UVAddr.
    // Doesn't check legality, since
    // copyin/copyout will do that.
    pub fn arg_addr(&self, n: usize) -> UVAddr {
        UVAddr::from(self.arg(n))
    }

    // Fetch the data at addr from the current process.
    // # Safety:
    // T memlayout is fixed
    pub unsafe fn fetch_data<T: ?Sized>(&mut self, addr: UVAddr, buf: &mut T) -> Result<usize, ()> {
        if addr.into_usize() >= self.data.sz || addr.into_usize() + size_of_val(buf) > self.data.sz
        {
            // both tests needed, in case of overflow
            return Err(());
        }
        self.data.uvm.as_mut().unwrap().copyin(buf, addr).and(Ok(0))
    }

    // Fetch the str at addr from the current process.
    // Return &str or error.
    pub fn fetch_str<'a>(&mut self, addr: UVAddr, buf: &'a mut [u8]) -> Result<&'a str, ()> {
        unsafe {
            self.fetch_data(addr, buf)?;
        }
        Ok(core::str::from_utf8_mut(buf)
            .or(Err(()))?
            .trim_end_matches(char::from(0)))
    }

    // Fetch the nth word-sized system call argument as a null-terminated string.
    // Copies into buf.
    // Return string length if OK (including nul), or Err
    pub fn arg_str<'a>(&mut self, n: usize, buf: &'a mut [u8]) -> Result<&'a str, ()> {
        self.fetch_str(self.arg_addr(n), buf)
    }

    fn call(&mut self) -> () {
        // Mapping syscall numbers to the function that handles the system call.
        if let Ok(res) = match SysCallNum::from_usize(self.tf.a7) {
            Some(SysCallNum::SysFork) => self.sys_fork(),
            Some(SysCallNum::SysExit) => self.sys_exit(),
            Some(SysCallNum::SysWait) => self.sys_wait(),
            Some(SysCallNum::SysPipe) => self.sys_pipe(),
            Some(SysCallNum::SysRead) => self.sys_read(),
            Some(SysCallNum::SysKill) => self.sys_kill(),
            Some(SysCallNum::SysExec) => self.sys_exec(),
            Some(SysCallNum::SysFstat) => self.sys_fstat(),
            Some(SysCallNum::SysChdir) => self.sys_chdir(),
            Some(SysCallNum::SysDup) => self.sys_dup(),
            Some(SysCallNum::SysGetpid) => self.sys_getpid(),
            Some(SysCallNum::SysSbrk) => self.sys_sbrk(),
            Some(SysCallNum::SysSleep) => self.sys_sleep(),
            Some(SysCallNum::SysUptime) => self.sys_uptime(),
            Some(SysCallNum::SysOpen) => self.sys_open(),
            Some(SysCallNum::SysWrite) => self.sys_write(),
            Some(SysCallNum::SysMknod) => self.sys_mknod(),
            Some(SysCallNum::SysUnlink) => self.sys_unlink(),
            Some(SysCallNum::SysLink) => self.sys_link(),
            Some(SysCallNum::SysMkdir) => self.sys_mkdir(),
            Some(SysCallNum::SysClose) => self.sys_close(),
            None => {
                println!("unknown sys call {}", self.tf.a7);
                Err(())
            }
        } {
            // Store system call return value in p->trapframe->a0
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
        tf = data.trapframe.unwrap().as_mut();
    }

    let mut syscalls = SysCalls { proc: p, data, tf };
    syscalls.call()
}
