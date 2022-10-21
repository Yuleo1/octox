use crate::{
    proc::{Process, PROCS},
    syscall::SysCalls,
    trap::TICKS,
};

impl SysCalls<'_> {
    pub fn sys_exit(&mut self) -> Result<usize, ()> {
        let n = self.arg(0) as i32;
        self.proc.exit(n)
    }

    pub fn sys_getpid(&self) -> Result<usize, ()> {
        Ok(self.proc.pid())
    }

    pub fn sys_fork(&self) -> Result<usize, ()> {
        self.proc.fork()
    }

    pub fn sys_wait(&self) -> Result<usize, ()> {
        let addr = self.arg_addr(0);
        self.proc.wait(addr).ok_or(())
    }

    pub fn sys_sbrk(&mut self) -> Result<usize, ()> {
        let n = self.arg(0) as isize;
        let addr = self.data.sz;
        match self.proc.grow_proc(n) {
            Err(_) => Err(()),
            Ok(_) => Ok(addr),
        }
    }

    pub fn sys_sleep(&self) -> Result<usize, ()> {
        let n = self.arg(0);
        let mut ticks = TICKS.lock();
        let ticks0 = *ticks;
        while *ticks - ticks0 < n {
            if self.proc.inner.lock().killed {
                return Err(());
            }
            ticks = self.proc.sleep(&(*ticks) as *const _ as usize, ticks);
        }
        Ok(0)
    }

    pub fn sys_kill(&self) -> Result<usize, ()> {
        let pid = self.arg(0);
        PROCS.kill(pid).and(Ok(0))
    }

    // return how many clock tick interrupts have occurred
    // since start.
    pub fn sys_uptime(&self) -> Result<usize, ()> {
        Ok(*TICKS.lock())
    }
}
