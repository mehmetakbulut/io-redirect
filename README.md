# io-redirect

[![Crates.io](https://img.shields.io/crates/v/io-redirect.svg)](https://crates.io/crates/io-redirect)
[![Documentation](https://docs.rs/io-redirect/badge.svg)](https://docs.rs/io-redirect)

A Rust library for redirecting file descriptors and handles such as stdout and stderr.

## Usage

```rust
use io_redirect::Redirectable;
use std::io::stdout;
use std::fs::File;

fn main() {
    let destination = OpenOptions::new()
        .write(true)
        .append(true)
        .open("/dev/kmsg")
        .unwrap();
    
    stdout().redirect(destination).unwrap();
}
```

## Contributing

This project is maintained by Mehmet Akbulut and everyone are welcome to contribute.

Feel free to open an issue or a pull request. Please do not submit any PRs with proprietary content.

## License

See [LICENSE.md](LICENSE.md).