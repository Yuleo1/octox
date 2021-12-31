# octox 

WIP. octox is an xv6-riscv like OS written in Rust.  

## Getting Started

### Requirements
* Install the rust toolchain to have cargo installed by following
  [this](https://www.rust-lang.org/tools/install) guide.
* Install `qemu-system-riscv`
* (option) Install `gdb-multiarch`

### Build and Run

* Add target(`rustup target add riscv64gc-unknown-none-elf`)
  ```
  git clone https://github.com/o8vm/octox.git
  cd octox
  rustup target add riscv64gc-unknown-none-elf
  ```
* Build: run `cargo build`.
* Run: run `cargo run`, then qemu will boot octox.  
  To exit, press `Ctrl+A` and `x`.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

This is a learning project for me, and I will not be accepting pull requests until I consider the implementation complete. However, discussions and advice are welcome.