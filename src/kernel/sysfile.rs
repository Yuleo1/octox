// Raw file descriptors
pub type RawFd = usize;

pub struct FileDesc(RawFd);

pub struct File(FileDesc);