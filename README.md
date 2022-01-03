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

---
[![Built with VSpaceCode](https://img.shields.io/badge/built%20with-VSpaceCode-brightgreen?logo=data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAABGdBTUEAALGPC/xhBQAAAHhlWElmTU0AKgAAAAgABQESAAMAAAABAAEAAAEaAAUAAAABAAAASgEbAAUAAAABAAAAUgEoAAMAAAABAAIAAIdpAAQAAAABAAAAWgAAAAAAAABIAAAAAQAAAEgAAAABAAKgAgAEAAAAAQAAABCgAwAEAAAAAQAAABAAAAAAiKeUQwAAAAlwSFlzAAALEwAACxMBAJqcGAAAAVlpVFh0WE1MOmNvbS5hZG9iZS54bXAAAAAAADx4OnhtcG1ldGEgeG1sbnM6eD0iYWRvYmU6bnM6bWV0YS8iIHg6eG1wdGs9IlhNUCBDb3JlIDYuMC4wIj4KICAgPHJkZjpSREYgeG1sbnM6cmRmPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5LzAyLzIyLXJkZi1zeW50YXgtbnMjIj4KICAgICAgPHJkZjpEZXNjcmlwdGlvbiByZGY6YWJvdXQ9IiIKICAgICAgICAgICAgeG1sbnM6dGlmZj0iaHR0cDovL25zLmFkb2JlLmNvbS90aWZmLzEuMC8iPgogICAgICAgICA8dGlmZjpPcmllbnRhdGlvbj4xPC90aWZmOk9yaWVudGF0aW9uPgogICAgICA8L3JkZjpEZXNjcmlwdGlvbj4KICAgPC9yZGY6UkRGPgo8L3g6eG1wbWV0YT4KGV7hBwAAA21JREFUOBFtU19MW2UU/33fd0uhqzAKvaX/F5uOCuVPtEAU5QHcXDY1zKjDxT0Zo76YbC/zzb74YGJifHCh0yczQxbdCG4mSwzEhWHsmDMO6mjZGH8bC7a0lfXP7b33816wyx48yflOzt+cc77fATQKI0x1+R+FbO1tETEYmBM7niqITkuRRhfnkVIiGB0LVYM0uZfzeHJfqG906OjL3Nzq42J7gNu6OrkI8Prz3/J9N+Z458gp7jFj9PEipKpcfOG9yV9bpMHJ7JqS3lhTTSYzzezkqWCoAeZiXAtUWlr9VBFtLJ/bnlq5ExvSc5n+OLs6RpdF+sbYyt1SitXWeBSZEYOB/iOViAAQuD1EdjpZUpZocnur1FRT5xetoj29uXmV4NDR0Ps+32yxWFDurz5ggiDA2mTFd9PX4G6yoyRXYDbWglEKSZbhbbRiOhVXoMqs3+DoYQ1Lix/X2azPbGezikO0s6sT4/hz6wGC3gBUrmK/yYzErd9gtOyHt9mGmVIOZwou5Yink32Tuan1f+7CvPbygZcOyy8eO8a/+PIcf+7QEa7bqnz67Fl+/OSpR/rXJz6UPyGHObyYE7Byz3fx0jhW1lO0OxhAviDB63Ljox+v4U4sBqgK3ho5ie/HJ/D2iTeRSNxD0ihQ1+sDMJ6O+wTrp2Huit7G4tIqGhsbcfnKBQwN9ONJnx8utxf6TjLZHJ7t60FbWzuoYMJC7A8EvAdBG5oIK3jdI2NXLosHrC3qwsJdOvzqMH76+QYanjDh+kwUa6vLqDHW4dLED1C1bmais0hn0uoHn31O5b9ux+HqaIs0tx/U55OqM/+vdFr2duDYlVJL0M893V0REuztDaUe5ma3YnHF93Q30zfvsIiwN4v4O5sBIQTr6RQY0SCj+aRKBbIGKllRmCTJPWxzYyM50Pu8ffiV4z3Tsd9L2l8LhHMtUML8+jLKpRLypSKKUhkFjUsVHV20RqsQ2VqIn38E5ejUzcnXzrwzKJXLSoVzNVsusgP1FlJnqEVyJ6MKhKoWDd5m0z6WSG1MPUws7UKZhsN7l9g32DukypUIoZTVGgwGh7me5spFUpCLxG9zMplwQ7EisfxOPlJN1nZFdzvQi2isagbYg60hReXvar5+bXwfo4RTwu5LsvRLWZG/ysWXbulxGunnrP4L032JENEvQdcAAAAASUVORK5CYII=)](https://vspacecode.github.io)
