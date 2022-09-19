#[cfg(target_os = "none")]
use crate::{
    fcntl::OpenOptions,
    fs::{IData, Inode},
    lazy::{SyncLazy, SyncOnceCell},
    param::{NDEV, NFILE},
    sleeplock::SleepLockGuard,
    spinlock::Mutex,
    stat::IType,
    vm::{UVAddr, VAddr}, array,
};
#[cfg(target_os = "none")]
use alloc::sync::Arc;
#[cfg(target_os = "none")]
use core::{cell::UnsafeCell, marker::PhantomData};

#[cfg(target_os = "none")]
pub static DEVSW: DevSW = DevSW::new();
#[cfg(target_os = "none")]
pub static FTABLE: SyncLazy<Mutex<[Option<Arc<VFile>>; NFILE]>> = SyncLazy::new(|| todo!());

#[cfg(target_os = "none")]
#[derive(Default, Clone)]
pub struct File {
    f: Option<Arc<VFile>>,
    readable: bool,
    writable: bool,
}

#[cfg(target_os = "none")]
pub enum VFile {
    Device(&'static dyn Device<UVAddr>),
    FsFile(FsFile<UVAddr>),
    Pipe,
}


// Device functions, map this trait using dyn
#[cfg(target_os = "none")]
pub trait Device<V: VAddr>: Send + Sync {
    fn read(&self, dst: &mut [u8]) -> Option<usize>;
    fn write(&self, src: &[u8]) -> Option<usize>;
    fn to_va(reference: &[u8]) -> V
    where
        Self: Sized,
    {
        V::from(reference.as_ptr() as usize)
    }
    fn major(&self) -> Major;
}

#[cfg(target_os = "none")]
impl core::fmt::Debug for dyn Device<UVAddr> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Device fn {:?}", self.major())
    }
}

#[cfg(target_os = "none")]
pub struct FsFile<V: VAddr> {
    off: UnsafeCell<u32>,
    ip: Inode,
    _maker: PhantomData<V>,
}

#[cfg(target_os = "none")]
type FTable = Mutex<[Option<Arc<VFile>>; NFILE]>;

#[cfg(target_os = "none")]
impl FTable {
    // Allocate a file structure
    pub fn alloc<'a>(
        &'a self,
        ip: Inode,
        ip_guard: SleepLockGuard<'a, IData>,
        opts: &OpenOptions,
    ) -> Option<(File, SleepLockGuard<IData>)> {
        let mut guard = self.lock();

        let mut empty: Option<&mut Option<Arc<VFile>>> = None;
        for f in guard.iter_mut() {
            match f {
                None if empty.is_none() => {
                    empty = Some(f);
                    break;
                }
                _ => continue,
            }
        }

        let f = empty?;
        f.replace(Arc::new(match ip_guard.itype {
            IType::Device => VFile::Device(DEVSW.get(ip_guard.major).unwrap()),
            IType::File => VFile::FsFile(FsFile::new(ip)),
            _ => unreachable!(),
        }));

        Some((
            File {
                f: f.clone(), // ref count = 2
                readable: opts.get_access_mode().0,
                writable: opts.get_access_mode().1,
            },
            ip_guard,
        ))
    }
}

#[cfg(target_os = "none")]
impl FsFile<UVAddr> {
    pub fn new(ip: Inode) -> Self {
        Self {
            off: UnsafeCell::new(0),
            ip,
            _maker: PhantomData,
        }
    }
    pub fn read(&self, dst: &mut [u8]) -> Option<usize> {
        let mut ip = self.ip.lock();
        let off = unsafe { *self.off.get() };
        match ip.read(From::from(Self::to_va(dst)), off, dst.len()) {
            // inode lock is held
            Ok(r) => {
                unsafe {
                    *self.off.get() += r as u32;
                }
                Some(r)
            }
            Err(_) => None,
        }
    }
    fn to_va(reference: &[u8]) -> UVAddr {
        UVAddr::from(reference.as_ptr() as usize)
    }
}

#[cfg(target_os = "none")]
pub struct DevSW {
    table: [SyncOnceCell<&'static dyn Device<UVAddr>>; NDEV],
}

#[cfg(target_os = "none")]
impl core::fmt::Debug for DevSW {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[")?;
        for (count, v) in self.table.iter().enumerate() {
            if count != 0 { write!(f, ", ")?; }
            if let Some(&v) = v.get() {
                write!(f, "{:?}", v)?;
            } else {
                write!(f, "None")?;
            } 
        }
        write!(f, "]")
    }
}

#[cfg(target_os = "none")]
impl DevSW {
    pub const fn new() -> Self {
        Self {
            table: array![SyncOnceCell::new(); NDEV],
        }
    }
    pub fn set(
        &self,
        devnum: Major,
        dev: &'static dyn Device<UVAddr>,
    ) -> Result<(), &'static (dyn Device<UVAddr> + 'static)> {
        self.table[devnum as usize].set(dev)
    }

    pub fn get(&self, devnum: Major) -> Option<&'static dyn Device<UVAddr>> {
        match self.table[devnum as usize].get() {
            Some(&dev) => Some(dev),
            None => None,
        }
    }
}

// Device Major Number
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Major {
    Null = 0,
    Console = 1,
}
impl Default for Major {
    fn default() -> Self {
        Self::Null
    }
}