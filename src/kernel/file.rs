use crate::vm::{UVAddr, VAddr};
use alloc::boxed::Box;
use core::cell::UnsafeCell;
use zerocopy::{AsBytes, FromBytes};

pub enum File {
    Device(Box<dyn Device<UVAddr, u8>>),
    FsFile(FsFile),
    Pipe,
}

// Device functions, map this trait using dyn
pub trait Device<V: VAddr, T: AsBytes + FromBytes>: Send + Sync {
    fn read(&self, dst: &mut [u8]) -> Option<usize>;
    fn write(&self, src: &[u8]) -> Option<usize>;
    fn to_va(reference: &T) -> V
    where
        Self: Sized,
    {
        V::from(reference as *const T as usize)
    }
}

pub struct FsFile {
    readable: bool,
    writable: bool,
    off: UnsafeCell<usize>,
}
