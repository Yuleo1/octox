use crate::fcntl::OMode;
use crate::file::{File, FTABLE, FType};
use crate::fs::{self, Path};
use crate::log::LOG;
use crate::param::MAXPATH;
use crate::stat::IType;
use crate::syscall::SysCalls;

// Raw file descriptors
pub type RawFd = usize;

impl SysCalls<'_> {
    // Fetch the nth usize system call argument as a file descpritor
    // and return both the descpritor and the corresponding struct file.
    pub fn arg_fd(&self, n: usize) -> Option<(RawFd, &File)> {
        let fd = self.arg(n);

        match self.data.ofile.get(fd)? {
            Some(f) => Some((fd, f)),
            None => None,
        }
    }

    // Allocate a file descpritor for the given file.
    // Takes over file from caller on success.
    pub fn fdalloc(&mut self, file: File) -> Option<RawFd> {
        for (fd, f) in self.data.ofile.iter_mut().enumerate() {
            if f.is_none() {
                f.replace(file);
                return Some(fd);
            }
        }
        None
    }

    pub fn sys_dup(&mut self) -> Result<usize, ()> {
        if let Some((_, f)) = self.arg_fd(0) {
            self.fdalloc(f.clone()).ok_or(())
        } else {
            Err(())
        }
    }

    pub fn sys_read(&mut self) -> Result<usize, ()> {
        let addr = self.arg_addr(1);
        let len = self.arg(2);

        let (_, f) = self.arg_fd(0).ok_or(())?;
        f.read(From::from(addr), len)
    }

    pub fn sys_write(&mut self) -> Result<usize, ()> {
        let addr = self.arg_addr(1);
        let len = self.arg(2);

        let (_, f) = self.arg_fd(0).ok_or(())?;
        f.write(From::from(addr), len)
    }

    pub fn sys_close(&mut self) -> Result<usize, ()> {
        let (fd, _) = self.arg_fd(0).ok_or(())?;
        let _f = self.data.ofile[fd].take().unwrap();
        Ok(0)
    }

    pub fn sys_fstat(&mut self) -> Result<usize, ()> {
        let st = self.arg_addr(0);
        let (_, f) = self.arg_fd(1).ok_or(())?;
        
        f.stat(From::from(st)).and(Ok(0))
    }

    // Create the path new as a link to the same inode as old.
    pub fn sys_link(&mut self) -> Result<usize, ()> {
        let mut old = [0; MAXPATH];
        let mut new = [0; MAXPATH];
        let old_path = Path::new(self.arg_str(0, &mut old)?);
        let new_path = Path::new(self.arg_str(1, &mut new)?);

        let res;
        {
            LOG.begin_op();
            res = fs::link(old_path, new_path);
            LOG.end_op();
        }
        res.and(Ok(0))
    }

    pub fn sys_unlink(&mut self) -> Result<usize, ()> {
        let mut path = [0; MAXPATH];
        let path = Path::new(self.arg_str(0, &mut path)?);

        let res;
        {
            LOG.begin_op();
            res = fs::unlink(&path);
            LOG.end_op();
        }
        res.and(Ok(0))
    }

    pub fn sys_open(&mut self) -> Result<RawFd, ()> {
        let mut path = [0u8; MAXPATH];
        let omode = self.arg(1);
        let path = Path::new(self.arg_str(0, &mut path)?);

        let fd;
        {
            LOG.begin_op();
            fd = FTABLE.alloc(OMode::from_usize(omode), FType::Node(path)).and_then(|f| self.fdalloc(f));
            LOG.end_op();
        }
        fd.ok_or(())
    }

    pub fn sys_mkdir(&mut self) -> Result<usize, ()> {
        let mut path = [0; MAXPATH];
        let path = Path::new(self.arg_str(0, &mut path)?);

        let res;
        {
            LOG.begin_op();
            res = fs::create(path, IType::Dir, 0, 0).and(Some(0)).ok_or(());
            LOG.end_op();
        }
        res
    }

    pub fn sys_mknod(&mut self) -> Result<usize, ()> {
        let mut path = [0u8; MAXPATH];
        let path = Path::new(self.arg_str(0, &mut path)?);
        let major = self.arg(1) as u16;
        let minor = self.arg(2) as u16;

        let res;
        {
            LOG.begin_op();
            res = fs::create(path, IType::Device, major, minor).and(Some(0)).ok_or(());
            LOG.end_op();
        }
        res
    }

    pub fn sys_chdir(&mut self) -> Result<usize, ()> {
        let mut path = [0u8; MAXPATH];
        let path = Path::new(self.arg_str(0, &mut path)?);

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
                self.data.cwd.replace(ip);
                Ok(0)
            };
            res = chidr();
            LOG.end_op();
        }
        res
    }
}
