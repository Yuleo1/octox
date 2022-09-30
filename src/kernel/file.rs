#[cfg(target_os = "none")]
use crate::array;
#[cfg(target_os = "none")]
use crate::fcntl::OMode;
use crate::fs::{Path, create};
#[cfg(target_os = "none")]
use crate::fs::{IData, Inode};
#[cfg(target_os = "none")]
use crate::lazy::{SyncLazy, SyncOnceCell};
#[cfg(target_os = "none")]
use crate::param::{NDEV, NFILE};
#[cfg(target_os = "none")]
use crate::pipe::Pipe;
#[cfg(target_os = "none")]
use crate::sleeplock::SleepLock;
#[cfg(target_os = "none")]
use crate::sleeplock::SleepLockGuard;
#[cfg(target_os = "none")]
use crate::spinlock::Mutex;
#[cfg(target_os = "none")]
use crate::stat::IType;
#[cfg(target_os = "none")]
use crate::vm::VirtAddr;
#[cfg(target_os = "none")]
use alloc::sync::Arc;
#[cfg(target_os = "none")]
use core::cell::UnsafeCell;

#[cfg(target_os = "none")]
pub static DEVSW: DevSW = DevSW::new();
#[cfg(target_os = "none")]
pub static FTABLE: SyncLazy<FTable> = SyncLazy::new(|| todo!());

#[cfg(target_os = "none")]
type FTable = Mutex<[Option<Arc<VFile>>; NFILE]>;

#[cfg(target_os = "none")]
#[derive(Default, Clone)]
pub struct File {
    f: Option<Arc<VFile>>,
    readable: bool,
    writable: bool,
}

#[cfg(target_os = "none")]
pub enum VFile {
    Device(&'static dyn Device),
    Inode(FsFD),
    Pipe(Pipe),
    None,
}

// Device functions, map this trait using dyn
#[cfg(target_os = "none")]
pub trait Device: Send + Sync {
    fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()>;
    fn write(&self, src: VirtAddr, n: usize) -> Result<usize, ()>;
    fn major(&self) -> Major;
}

#[cfg(target_os = "none")]
impl core::fmt::Debug for dyn Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Device fn {:?}", self.major())
    }
}

#[cfg(target_os = "none")]
pub struct FsFD {
    off: UnsafeCell<u32>,
    ip: Inode,
}

#[cfg(target_os = "none")]
impl FsFD {
    pub fn new(ip: Inode) -> Self {
        Self {
            off: UnsafeCell::new(0),
            ip,
        }
    }
    pub fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()> {
        let mut ip = self.ip.lock();
        let off = unsafe { &mut *self.off.get() };

        match ip.read(dst, *off, n) {
            // inode lock is held
            Ok(r) => {
                *off += r as u32;
                Ok(r)
            }
            Err(_) => Err(()),
        }
    }
}

#[cfg(target_os = "none")]
impl VFile {
    fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()> {
        match self {
            VFile::Device(d) => d.read(dst, n),
            VFile::Inode(f) => f.read(dst, n),
            _ => Err(()),
        }
    }
}

#[cfg(target_os = "none")]
impl File {
    // Read from file.
    pub fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()> {
        if !self.readable {
            return Err(());
        }
        self.f.as_ref().unwrap().read(dst, n)
    }

    // Write to file f.
    pub fn write(&self, src: VirtAddr, n: usize) -> Result<usize, ()> {
        if !self.writable {
            return Err(());
        }
        todo!()
    }
}

// File Allocation Type Source
#[cfg(target_os = "none")]
pub enum FType<'a> {
    Node(&'a Path),
    Pipe(Pipe),
}

impl<'a> FType<'a> {
    pub fn from_path(path: &'a Path) -> Self {
        Self::Node(path)
    }
    pub fn from_pipe(pipe: Pipe) -> Self {
        Self::Pipe(pipe)
    }
}

#[cfg(target_os = "none")]
impl FTable {
    // Allocate a file structure
    pub fn alloc<'a, 'b>(
        &'a self,
        opts: OMode,
        ftype: FType<'b>,
    ) -> Option<File> {

        let inner: Arc<VFile> = Arc::new(match ftype {
            FType::Node(path) => { 
                let ip: Inode;
                let mut ip_guard: SleepLockGuard<'_, IData>;

                if opts.is_create() {
                    ip = create(path, IType::File, 0, 0)?;
                    ip_guard = ip.lock();
                } else {
                    (_, ip) = path.namei()?;
                    ip_guard = ip.lock();
                    if ip_guard.itype() == IType::Dir && !opts.is_rdonly() {
                        return None;
                    }
                }
                // todo 
                match ip_guard.itype() {
                    IType::Device if ip_guard.major() != Major::Invalid && ip_guard.major() != Major::Null => {
                        VFile::Device(DEVSW.get(ip_guard.major()).unwrap())
                    },
                    IType::Dir | IType::File => {
                        if opts.is_trunc() && ip_guard.itype() == IType::File {
                            ip_guard.trunc();
                        }
                        SleepLock::unlock(ip_guard);
                        VFile::Inode(FsFD::new(ip))
                    },
                    _ => return None,
                }
            },
            FType::Pipe(pi) => {
                VFile::Pipe(pi)
            },
        });

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
        f.replace(inner);
        Some(
            File {
                f: f.clone(), // ref count = 2
                readable: opts.is_read(),
                writable: opts.is_write(),
            }
        )
    }
}

#[cfg(target_os = "none")]
pub struct DevSW {
    table: [SyncOnceCell<&'static dyn Device>; NDEV],
}

#[cfg(target_os = "none")]
impl core::fmt::Debug for DevSW {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[")?;
        for (count, v) in self.table.iter().enumerate() {
            if count != 0 {
                write!(f, ", ")?;
            }
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
        dev: &'static dyn Device,
    ) -> Result<(), &'static (dyn Device + 'static)> {
        self.table[devnum as usize].set(dev)
    }

    pub fn get(&self, devnum: Major) -> Option<&'static dyn Device> {
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
    Invalid,
}
impl Default for Major {
    fn default() -> Self {
        Self::Null
    }
}

impl Major {
    pub fn from_usize(bits: u16) -> Major {
        match bits {
            0 => Major::Null,
            1 => Major::Console,
            _ => Major::Invalid,
        }
    }
}