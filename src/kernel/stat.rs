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
