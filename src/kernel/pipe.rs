use crate::{
    fcntl::OMode,
    file::{FType, File, FTABLE},
    mpmc::*,
    proc::{CopyInOut, CPUS},
    vm::VirtAddr,
};

#[derive(Debug)]
pub struct Pipe {
    rx: Option<SyncReceiver<u8>>,
    tx: Option<SyncSender<u8>>,
}

impl Pipe {
    const PIPESIZE: usize = 512;

    pub fn new(rx: Option<SyncReceiver<u8>>, tx: Option<SyncSender<u8>>) -> Self {
        Self { rx, tx }
    }

    pub fn get_mode(&self) -> OMode {
        let mut omode = OMode::new();
        omode.read(self.rx.is_some()).write(self.tx.is_some());
        omode
    }

    pub fn alloc() -> Option<(File, File)> {
        let (tx, rx) = sync_channel::<u8>(Self::PIPESIZE, "pipe");

        let p0 = Self::new(Some(rx), None);
        let p1 = Self::new(None, Some(tx));
        let f0 = FTABLE.alloc(p0.get_mode(), FType::Pipe(p0))?;
        let f1 = FTABLE.alloc(p1.get_mode(), FType::Pipe(p1))?;

        Some((f0, f1))
    }

    pub fn write(&self, src: VirtAddr, n: usize) -> Result<usize, ()> {
        let p = CPUS.my_proc().unwrap();

        let tx = self.tx.as_ref().ok_or(())?;

        let mut i = 0;
        while i < n {
            let mut ch: u8 = 0;
            if unsafe { p.either_copyin(&mut ch, src) }.is_err() {
                break;
            }
            tx.send(ch);
            i += 1;
        }
        Ok(i)
    }

    pub fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()> {
        let p = CPUS.my_proc().unwrap();

        let rx = self.rx.as_ref().ok_or(())?;

        let mut i = 0;
        while i < n {
            let ch = rx.recv();
            if unsafe { p.either_copyout(dst, &ch) }.is_err() {
                break;
            }
            i += 1;
        }
        Ok(i)
    }
}
