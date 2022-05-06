use alloc::sync::Arc;
use crate::{vm::{UVAddr, VAddr}, fs::{Inode, IData}, param::{NDEV, NFILE}, spinlock::Mutex, lazy::SyncLazy, sleeplock::SleepLockGuard, fcntl::OpenOptions, stat::IType};
use core::{cell::UnsafeCell, marker::PhantomData};

pub static DEVSW: Mutex<DevSW> = Mutex::new(DevSW::new(), "devsw");
pub static FTABLE: SyncLazy<Mutex<[Option<Arc<VFile>>; NFILE]>> = SyncLazy::new(|| todo!());

#[derive(Default)]
pub struct File {
    f: Option<Arc<VFile>>,
    readable: bool,
    writable: bool,
}
pub enum VFile {
    Device(&'static dyn Device<UVAddr>),
    FsFile(FsFile<UVAddr>),
    Pipe,
}

// Device functions, map this trait using dyn
pub trait Device<V: VAddr>: Send + Sync {
    fn read(&self, dst: &mut [u8]) -> Option<usize>;
    fn write(&self, src: &[u8]) -> Option<usize>;
    fn to_va(reference: &[u8]) -> V
    where
       Self: Sized,
    {
        V::from(reference.as_ptr() as usize)
    }
}

pub struct FsFile<V: VAddr> {
    off: UnsafeCell<u32>,
    ip: Inode,
    _maker: PhantomData<V>,
}

type FTable = Mutex<[Option<Arc<VFile>>; NFILE]>;

impl FTable {
    // Allocate a file structure
    pub fn alloc(&self, ip: Inode, ip_guard: SleepLockGuard<IData>, opts: &OpenOptions) -> Option<File> {
        let mut guard = self.lock();

        let mut empty: Option<&mut Option<Arc<VFile>>> = None;
        for f in guard.iter_mut() {
            match f {
                None if empty.is_none() => { 
                    empty = Some(f);
                    break;
                },
                _ => continue,
            }
        }
        let empty = match empty {
            Some(f) => f,
            None => return None,
        };

        let vfile = match ip_guard.itype {
            IType::Device => VFile::Device(DEVSW.lock().get(ip_guard.major).unwrap()),
            IType::File => VFile::FsFile(FsFile::new(ip)),
            _ => unreachable!(),
        };

        todo!()
    }
}


impl FsFile<UVAddr> {
    pub fn new(ip: Inode) -> Self {
        Self {
            off: UnsafeCell::new(0),
            ip,
            _maker: PhantomData
        }
    }
    pub fn read(&self, dst: &mut [u8]) -> Option<usize> {
        let mut ip = self.ip.lock();
        let off = unsafe { *self.off.get() };
        match ip.read(From::from(Self::to_va(dst)), off, dst.len()) { // inode lock is held
            Ok(r) => {
                unsafe {
                    *self.off.get() += r as u32;
                }
                Some(r)
            },
            Err(_) => {
                None
            }
        }
    }
    fn to_va(reference: &[u8]) -> UVAddr {
        UVAddr::from(reference.as_ptr() as usize)
    }
}

pub struct DevSW {
    table: [Option<&'static dyn Device<UVAddr>>; NDEV],
}

impl DevSW {
    pub const fn new() -> Self {
        Self {
            table: [None; NDEV]
        }
    }
    pub fn get(&self, devnum: Major) -> Option<&'static dyn Device<UVAddr>> {
        self.table[devnum as usize]
    }
    pub fn get_mut(&mut self, devnum: Major) -> &mut Option<&'static dyn Device<UVAddr>> {
        &mut self.table[devnum as usize]
    }
}

impl Default for Major {
    fn default() -> Self {
        Self::Null
    }
}

// Device Major Number
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Major {
    Null = 0,
    Console = 1,
}

