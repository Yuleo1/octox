pub enum OMode {
    RDONLY = 0x000,
    WRONLY = 0x001,
    RDWR = 0x002,
    CREATE = 0x200,
    TRUNC = 0x400,
}

impl OMode {
    pub fn from_usize(bits: usize) -> Option<Self> {
        match bits {
            0x000 => Some(Self::RDONLY),
            0x001 => Some(Self::WRONLY),
            0x002 => Some(Self::RDWR),
            0x200 => Some(Self::CREATE),
            0x400 => Some(Self::TRUNC),
            _ => None,
        }
    }

    pub fn is_readable(&self) -> bool {
        match self {
            &Self::RDONLY | &Self::RDWR => true,
            _ => false,
        }
    }

    pub fn is_writable(&self) -> bool {
        match self {
            &Self::WRONLY | &Self::RDWR => true,
            _ => false,
        }
    }
}
