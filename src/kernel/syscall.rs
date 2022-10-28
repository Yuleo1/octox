#[cfg(target_os = "none")]
use crate::{
    array,
    exec::exec,
    fcntl::OMode,
    file::{FType, File, FTABLE},
    fs::{self, Path},
    log::LOG,
    param::{MAXARG, MAXPATH},
    pipe::Pipe,
    proc::{ProcData, Process, CPUS, PROCS},
    riscv::PGSIZE,
    stat::IType,
    trap::TICKS,
    vm::{Addr, UVAddr},
};

#[cfg(target_os = "none")]
use alloc::string::{String, ToString};
#[cfg(target_os = "none")]
use core::concat;
use core::mem::variant_count;
#[cfg(target_os = "none")]
use core::mem::{size_of, size_of_val};

#[derive(Copy, Clone, Debug)]
#[repr(usize)]
pub enum SysCalls {
    Fork = 1,
    Exit = 2,
    Wait = 3,
    Pipe = 4,
    Read = 5,
    Kill = 6,
    Exec = 7,
    Fstat = 8,
    Chdir = 9,
    Dup = 10,
    Getpid = 11,
    Sbrk = 12,
    Sleep = 13,
    Uptime = 14,
    Open = 15,
    Write = 16,
    Mknod = 17,
    Unlink = 18,
    Link = 19,
    Mkdir = 20,
    Close = 21,
    Invalid = 0,
}

impl SysCalls {
    const TABLE: [(fn() -> Result<usize, ()>, &'static str); variant_count::<Self>()] = [
        (Self::invalid, ""),
        (Self::fork, "() -> usize"), // fork: Create a process, return child's PID.
        (Self::exit, "(xstatus: i32) -> !"), // exit: Terminate the current process; status reported to wait(). No Return.
        (Self::wait, "(xstatus: &mut i32) -> usize"), // wait: Wait for a child to exit; exit status in &status; retunrs child PID.
        (Self::pipe, "(p: &mut [usize]) -> usize"), // pipe: Create a pipe, put read/write file descpritors in p[0] and p[1].
        (Self::read, "(fd: usize, b: *mut u8, n: usize) -> usize"), // read: Read n bytes into buf; returns number read; or 0 if end of file
        (Self::kill, "(pid: usize) -> usize"), // kill: Terminate process PID. Returns 0, or -1 for Error
        (Self::exec, "(filename: &str, argv: &[&str]) -> usize"), // exec: Load a file and execute it with arguments; only returns if error.
        (Self::fstat, "(fd: usize, st: &mut stat) -> usize"), // fstat: Place info about an open file into st.
        (Self::chdir, "(dirname: &str) -> usize"), // chdir: Change the current directory.
        (Self::dup, "(fd: usize) -> usize"), // dup: Return a new file descpritor referring to the same file as fd.
        (Self::getpid, "() -> usize"),       // getpid: Return the current process's PID.
        (Self::sbrk, "(n: usize) -> usize"), // sbrk: Grow process's memory by n bytes. Returns start fo new memory.
        (Self::sleep, "(n: usize) -> usize"), // sleep: Pause for n clock ticks.
        (Self::uptime, "() -> usize"),       // uptime: Return how many clock ticks since start.
        (Self::open, "(filename: &str, flags: usize) -> usize"), // open: Open a file; flags indicate read/write; returns an fd.
        (Self::write, "(fd: usize, b: *const u8, n: usize) -> usize"), // write: Write n bytes from buf to file descpritor fd; returns n.
        (Self::mknod, "(file: &str, mj: usize, mi: usize) -> usize"), // mknod: Create a device file
        (Self::unlink, "(file: &str) -> usize"),                      // unlink: Remove a file
        (Self::link, "(file1: &str, file2: &str) -> usize"), // link: Create another name (file2) for the file file1.
        (Self::mkdir, "(dir: &str) -> usize"),               // mkdir: Create a new directory.
        (Self::close, "(fd: usize) -> usize"),               // close: Release open file fd.
    ];
    fn invalid() -> Result<usize, ()> {
        unreachable!()
    }
}

#[cfg(target_os = "none")]
pub fn syscall() {
    let p = CPUS.my_proc().unwrap();
    let data = unsafe { &mut *p.data.get() };
    let tf = data.trapframe.as_mut().unwrap();
    let syscall_id = SysCalls::from_usize(tf.a7);
    tf.a0 = match syscall_id {
        SysCalls::Invalid => {
            println!("{} {}: unknown sys call {}", p.pid(), data.name, tf.a7);
            -1 as isize as usize
        }
        _ => SysCalls::TABLE[syscall_id as usize].0().unwrap_or(-1 as isize as usize),
    }
}

#[cfg(target_os = "none")]
type RawFd = usize;
#[cfg(target_os = "none")]
impl ProcData {
    pub fn arg(&self, n: usize) -> usize {
        let tf = self.trapframe.as_ref().unwrap();
        match n {
            0 => tf.a0,
            1 => tf.a1,
            2 => tf.a2,
            3 => tf.a3,
            4 => tf.a4,
            5 => tf.a5,
            _ => panic!("arg"),
        }
    }

    // Retrive an argument as a UVAddr.
    // Doesn't check legality, since
    // copyin/copyout will do that.
    pub fn arg_addr(&self, n: usize) -> UVAddr {
        UVAddr::from(self.arg(n))
    }

    // Fetch the data at addr from the current process.
    // Safety: if T memlayout is fixed
    pub unsafe fn fetch_data<T: ?Sized>(&mut self, addr: UVAddr, buf: &mut T) -> Result<usize, ()> {
        if addr.into_usize() >= self.sz || addr.into_usize() + size_of_val(buf) > self.sz {
            // both tests needed, in case of overflow
            return Err(());
        }
        self.uvm.as_mut().unwrap().copyin(buf, addr).and(Ok(0))
    }

    // Fetch the str at addr from the current process.
    // Return &str or Err
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

    // Fetch the nth usize system call argument as a file descpritor
    // and return both the descpritor and the corresponding struct file.
    pub fn arg_fd(&self, n: usize) -> Option<(RawFd, &File)> {
        let fd = self.arg(n);

        match self.ofile.get(fd)? {
            Some(f) => Some((fd, f)),
            None => None,
        }
    }

    // Allocate a file descpritor for the given file.
    // Takes over file from caller on success.
    pub fn fdalloc(&mut self, file: File) -> Option<RawFd> {
        for (fd, f) in self.ofile.iter_mut().enumerate() {
            if f.is_none() {
                f.replace(file);
                return Some(fd);
            }
        }
        None
    }
}

// Process related system calls
impl SysCalls {
    fn exit() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let p = CPUS.my_proc().unwrap();
            let n = p.data().arg(0) as i32;
            p.exit(n)
            // not reached
        }
    }
    fn getpid() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            Ok(CPUS.my_proc().unwrap().pid())
        }
    }
    fn fork() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            CPUS.my_proc().unwrap().fork()
        }
    }
    fn wait() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let p = CPUS.my_proc().unwrap();
            let addr = p.data().arg_addr(0);
            p.wait(addr).ok_or(())
        }
    }
    fn sbrk() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let p = CPUS.my_proc().unwrap();
            let n = p.data().arg(0) as isize;
            let addr = p.data().sz;
            p.grow_proc(n).and(Ok(addr))
        }
    }
    fn sleep() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let p = CPUS.my_proc().unwrap();
            let n = p.data().arg(0);
            let mut ticks = TICKS.lock();
            let ticks0 = *ticks;
            while *ticks - ticks0 < n {
                if p.inner.lock().killed {
                    return Err(());
                }
                ticks = p.sleep(&(*ticks) as *const _ as usize, ticks);
            }
            Ok(0)
        }
    }
    fn kill() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let p = CPUS.my_proc().unwrap();
            let pid = p.data().arg(0);
            PROCS.kill(pid).and(Ok(0))
        }
    }
    fn uptime() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            Ok(*TICKS.lock())
        }
    }
}

// System Calls related to File operations
impl SysCalls {
    fn dup() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let data = CPUS.my_proc().unwrap().data_mut();
            if let Some((_, f)) = data.arg_fd(0) {
                data.fdalloc(f.clone()).ok_or(())
            } else {
                Err(())
            }
        }
    }
    fn read() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let data = CPUS.my_proc().unwrap().data();
            let addr = data.arg_addr(1);
            let len = data.arg(2);

            let (_, f) = data.arg_fd(0).ok_or(())?;
            f.read(From::from(addr), len)
        }
    }
    fn write() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let data = CPUS.my_proc().unwrap().data();
            let addr = data.arg_addr(1);
            let len = data.arg(2);

            let (_, f) = data.arg_fd(0).ok_or(())?;
            f.write(From::from(addr), len)
        }
    }
    fn close() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let data = CPUS.my_proc().unwrap().data_mut();
            let (fd, _) = data.arg_fd(0).ok_or(())?;
            let _f = data.ofile[fd].take().unwrap();
            Ok(0)
        }
    }
    fn fstat() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let data = CPUS.my_proc().unwrap().data();
            let st = data.arg_addr(0);
            let (_, f) = data.arg_fd(1).ok_or(())?;

            f.stat(From::from(st)).and(Ok(0))
        }
    }
    fn link() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let mut old = [0; MAXPATH];
            let mut new = [0; MAXPATH];
            let data = CPUS.my_proc().unwrap().data_mut();
            let old_path = Path::new(data.arg_str(0, &mut old)?);
            let new_path = Path::new(data.arg_str(1, &mut new)?);

            let res;
            {
                LOG.begin_op();
                res = fs::link(old_path, new_path);
                LOG.end_op();
            }
            res.and(Ok(0))
        }
    }
    fn unlink() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let mut path = [0; MAXPATH];
            let data = CPUS.my_proc().unwrap().data_mut();
            let path = Path::new(data.arg_str(0, &mut path)?);

            let res;
            {
                LOG.begin_op();
                res = fs::unlink(&path);
                LOG.end_op();
            }
            res.and(Ok(0))
        }
    }
    fn open() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let mut path = [0u8; MAXPATH];
            let data = CPUS.my_proc().unwrap().data_mut();
            let omode = data.arg(1);
            let path = Path::new(data.arg_str(0, &mut path)?);

            let fd;
            {
                LOG.begin_op();
                fd = FTABLE
                    .alloc(OMode::from_usize(omode), FType::Node(path))
                    .and_then(|f| data.fdalloc(f));
                LOG.end_op();
            }
            fd.ok_or(())
        }
    }
    fn mkdir() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let mut path = [0u8; MAXPATH];
            let path = Path::new(CPUS.my_proc().unwrap().data_mut().arg_str(0, &mut path)?);

            let res;
            {
                LOG.begin_op();
                res = fs::create(path, IType::Dir, 0, 0).and(Some(0)).ok_or(());
                LOG.end_op();
            }
            res
        }
    }
    fn mknod() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let mut path = [0u8; MAXPATH];
            let data = CPUS.my_proc().unwrap().data_mut();
            let path = Path::new(data.arg_str(0, &mut path)?);
            let major = data.arg(1) as u16;
            let minor = data.arg(2) as u16;

            let res;
            {
                LOG.begin_op();
                res = fs::create(path, IType::Device, major, minor)
                    .and(Some(0))
                    .ok_or(());
                LOG.end_op();
            }
            res
        }
    }
    fn chdir() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let mut path = [0u8; MAXPATH];
            let data = CPUS.my_proc().unwrap().data_mut();
            let path = Path::new(data.arg_str(0, &mut path)?);

            let res;
            {
                LOG.begin_op();
                let mut chidr = || -> Result<usize, ()> {
                    let (_, ip) = path.namei().ok_or(())?;
                    {
                        let ip_guard = ip.lock();
                        if ip_guard.itype() != IType::Dir {
                            return Err(());
                        }
                    }
                    data.cwd.replace(ip);
                    Ok(0)
                };
                res = chidr();
                LOG.end_op();
            }
            res
        }
    }
    fn exec() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let mut path = [0u8; MAXPATH];
            let data = CPUS.my_proc().unwrap().data_mut();
            let path = Path::new(data.arg_str(0, &mut path)?);

            let mut argv: [Option<String>; MAXARG] = array![None; MAXARG];
            let uargv = data.arg_addr(1);
            let mut uarg: UVAddr = From::from(0);
            let mut uarg_cp_buf: [u8; PGSIZE] = [0u8; PGSIZE];

            let mut i = 0;
            loop {
                match argv.get_mut(i) {
                    None => return Err(()),
                    Some(argvi) => {
                        unsafe { data.fetch_data(uargv + size_of::<usize>() * i, &mut uarg) }?;

                        if uarg.into_usize() == 0 {
                            break;
                        }

                        argvi.replace(data.fetch_str(uarg, &mut uarg_cp_buf)?.to_string());
                        i += 1;
                    }
                }
            }

            exec(path, argv)
        }
    }
    fn pipe() -> Result<usize, ()> {
        #[cfg(not(target_os = "none"))]
        unimplemented!();
        #[cfg(target_os = "none")]
        {
            let data = CPUS.my_proc().unwrap().data_mut();
            let fdarr: UVAddr = data.arg_addr(0); // user pointer to array to two integers

            let (rf, wf) = Pipe::alloc().ok_or(())?;
            let fd0 = data.fdalloc(rf).ok_or(())?;
            let fd1 = match data.fdalloc(wf) {
                Some(fd) => fd,
                _ => {
                    data.ofile[fd0].take();
                    return Err(());
                }
            };

            let uvm = data.uvm.as_mut().unwrap();
            if unsafe {
                uvm.copyout(fdarr, &fd0).is_err()
                    || uvm.copyout(fdarr + size_of::<usize>(), &fd1).is_err()
            } {
                data.ofile[fd0].take();
                data.ofile[fd1].take();
                return Err(());
            }
            Ok(0)
        }
    }
}

impl SysCalls {
    fn from_usize(n: usize) -> Self {
        match n {
            1 => Self::Fork,
            2 => Self::Exit,
            3 => Self::Wait,
            4 => Self::Pipe,
            5 => Self::Read,
            6 => Self::Kill,
            7 => Self::Exec,
            8 => Self::Fstat,
            9 => Self::Chdir,
            10 => Self::Dup,
            11 => Self::Getpid,
            12 => Self::Sbrk,
            13 => Self::Sleep,
            14 => Self::Uptime,
            15 => Self::Open,
            16 => Self::Write,
            17 => Self::Mknod,
            18 => Self::Unlink,
            19 => Self::Link,
            20 => Self::Mkdir,
            21 => Self::Close,
            _ => Self::Invalid,
        }
    }
}

// Generate system call interface for userland
#[cfg(not(target_os = "none"))]
impl SysCalls {
    fn into_enum_iter() -> std::vec::IntoIter<SysCalls> {
        (0..core::mem::variant_count::<SysCalls>())
            .map(|i| SysCalls::from_usize(i))
            .collect::<Vec<SysCalls>>()
            .into_iter()
    }
    fn signature(self) -> String {
        format!(
            "extern \"C\" fn {}{}",
            self.fn_name(),
            Self::TABLE[self as usize].1,
        )
    }
    fn fn_name(&self) -> String {
        format!("{:?}", self).to_lowercase()
    }

    fn gen_usys(self) -> String {
        format!(
            r#"#[naked]
#[no_mangle]
pub {} {{
    unsafe {{
        asm!(
            "li a7, {{syscall}}",
            "ecall",
            "ret",
            syscall = const {},
            options(noreturn),
        );
    }}
}}

"#,
            self.signature(),
            self as usize,
        )
    }
}
