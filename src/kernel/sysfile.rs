use crate::fcntl::OMode;
use crate::file::File;
use crate::fs::Path;
use crate::log::LOG;
use crate::param::MAXPATH;
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

        if let Some((_, f)) = self.arg_fd(0) {
            f.read(From::from(addr), len)
        } else {
            Err(())
        }
    }

    pub fn sys_open(&mut self) -> Result<usize, ()> {
        let mut path = [0u8; MAXPATH];
        let omode = self.arg(1);
        let path = Path::new(self.arg_str(0, &mut path)?);

        LOG.begin_op();

        match OMode::from_usize(omode) {
            Some(OMode::CREATE) => {
                todo!()
            }
            _ => {
                //if let Some(_) = namei(path) {
                //todo!()
                //} else {
                //  LOG.end_op();
                //return Err(());
                //}
            }
        }
        Err(())
    }
}
