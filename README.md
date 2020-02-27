# Saphir
[![doc](https://docs.rs/saphir/badge.svg)](https://docs.rs/saphir/)
[![crate](https://img.shields.io/crates/v/saphir.svg)](https://crates.io/crates/saphir)
[![issue](https://img.shields.io/github/issues/richerarc/saphir.svg)](https://github.com/richerarc/saphir/issues)
![Rust](https://github.com/richerarc/saphir/workflows/Rust/badge.svg?branch=master)
![downloads](https://img.shields.io/crates/d/saphir.svg)
[![license](https://img.shields.io/crates/l/saphir.svg)](https://github.com/richerarc/saphir/blob/master/LICENSE)
[![dependency status](https://deps.rs/repo/github/richerarc/saphir/status.svg)](https://deps.rs/repo/github/richerarc/saphir)

### Saphir is a fully async-await http server framework for rust
The goal is to give low-level control to your web stack (as hyper does) without the time consuming task of doing everything from scratch.

## Quick server setup
```rust
use saphir::prelude::*;


async fn test_handler(mut req: Request<Body>) -> (u16, Option<String>) {
    (200, req.captures_mut().remove("variable"))
}

#[tokio::main]
async fn main() -> Result<(), SaphirError> {
    env_logger::init();

    let server = Server::builder()
        .configure_listener(|l| {
            l.interface("127.0.0.1:3000")
        })
        .configure_router(|r| {
            r.route("/{variable}/print", Method::GET, test_handler)
        })
        .build().await?;

    server.run().await
}
```
