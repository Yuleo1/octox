use crate::{mpmc::*, file::File};


pub struct Pipe<T: Send> {
    read: Option<SyncReceiver<T>>,
    write: Option<SyncSender<T>>,
}

impl<T: Send> Pipe<T> {
    pub fn alloc() -> (Option<File>, Option<File>) {
        todo!()
    }
}