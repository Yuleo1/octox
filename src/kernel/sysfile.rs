use crate::file::File;
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
    // Takes over file reference from caller on success.
    pub fn fdalloc(&mut self, file: File) -> Option<RawFd> {
        for (fd, f) in self.data.ofile.iter_mut().enumerate() {
            if f.is_none() {
                f.replace(file);
                return Some(fd);
            }
        }
        None
    }

    pub fn dup(&mut self) -> Result<usize, ()>{
        if let Some((_, f)) = self.arg_fd(0) {
            self.fdalloc(f.clone()).ok_or(())
        } else {
            Err(())
        }
    }

}
