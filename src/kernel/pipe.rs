use crate::{mpmc::*, file::{File, FTABLE, FType}, fcntl::OMode, vm::VirtAddr, proc::CPUS};

pub struct Pipe {
    rx: Option<SyncReceiver<u8>>,
    tx: Option<SyncSender<u8>>,
}

impl Pipe {
    const PIPESIZE: usize = 512;
    
    pub fn new(rx: Option<SyncReceiver<u8>>, tx: Option<SyncSender<u8>>) -> Self {
        Self {
            rx,
            tx,
        }
    }

    pub fn get_mode(&self) -> OMode {
        let mut omode = OMode::new();
        omode.read(self.rx.is_some()).write(self.tx.is_some());
        omode
    }

    pub fn alloc() -> Result<(File, File), ()> {
        let (tx, rx) = sync_channel::<u8>(Self::PIPESIZE, "pipe");

        let p0 = Self::new(Some(rx), None);
        let p1 = Self::new(None, Some(tx));
        let f0 = FTABLE.alloc(p0.get_mode(), FType::from_pipe(p0)).ok_or(())?;
        let f1 = FTABLE.alloc(p1.get_mode(), FType::from_pipe(p1)).ok_or(())?;
        
        Ok((f0, f1))
    }

    pub fn read(&self, dst: VirtAddr, n: usize) -> Result<usize, ()> {
        let p = CPUS.my_proc().unwrap();

        let rx = self.rx.as_ref().ok_or(())?;

        let mut i = 0;
        for idx in 0..n {
            let ch = rx.recv();
            todo!()
            //i = idx;
        }
        Ok(i)
    }
}