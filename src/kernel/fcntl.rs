mod omode {
    pub const _RDONLY: usize = 0x000;
    pub const WRONLY: usize = 0x001;
    pub const RDWR: usize = 0x002;
    pub const CREATE: usize = 0x200;
    pub const TRUNC: usize = 0x400;
}

pub struct OMode {
    read: bool,
    write: bool,
    truncate: bool,
    create: bool,
}

impl OMode {
    pub fn new() -> Self {
        Self {
            read: false,
            write: false,
            truncate: false,
            create: false,
        }
    }

    pub fn read(&mut self, read: bool) -> &mut Self {
        self.read = read;
        self
    }
    pub fn write(&mut self, write: bool) -> &mut Self {
        self.write = write;
        self
    }
    fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.truncate = truncate;
        self
    }
    fn create(&mut self, create: bool) -> &mut Self {
        self.create = create;
        self
    }

    pub fn from_usize(bits: usize) -> Self {
        let mut mode = Self::new();
        mode.read(true)
            .read(bits & omode::WRONLY == 0)
            .write(bits & omode::WRONLY != 0 || bits & omode::RDWR != 0)
            .create(bits & omode::CREATE != 0)
            .truncate(bits & omode::TRUNC != 0);
        mode
    }

    pub fn is_read(&self) -> bool {
        self.read
    }

    pub fn is_write(&self) -> bool {
        self.write
    }

    pub fn is_create(&self) -> bool {
        self.create
    }

    pub fn is_trunc(&self) -> bool {
        self.truncate
    }

    pub fn is_rdonly(&self) -> bool {
        self.read && !self.write
    }
}
