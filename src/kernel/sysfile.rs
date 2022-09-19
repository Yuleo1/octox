use crate::{file, proc::CPUS};
use crate::syscall::SysCalls;

// Raw file descriptors
pub type RawFd = usize;

pub struct FileDesc(RawFd);

pub struct File(FileDesc);

impl FileDesc {
    // Allocate a file descriptor for the given file.
    // Takes over file reference from caller on success.
    pub fn alloc(file: file::File) -> Option<Self> {
        let p = CPUS.my_proc().unwrap();
        let data = unsafe { &mut *p.data.get() };
        for (fd, f) in data.ofile.iter_mut().enumerate() {
            if f.is_none() {
                f.replace(file);
                return Some(FileDesc(fd));
            }
        }
        None
    }
}


impl SysCalls<'_> {
    pub fn hoge(&self) -> () {
        ()
    }
}
