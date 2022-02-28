#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IType {
    None = 0,
    Dir = 1,
    File = 2,
    Device = 3,
}

impl Default for IType {
    fn default() -> Self {
        IType::None
    }
}

pub struct Stat {
    pub dev: u32,     // File system's disk device
    pub ino: u32,     // Inode number
    pub itype: IType, // Type of file
    pub nlink: u16,   // Number of links to file
    pub size: usize,  // Size of file in bytes
}
