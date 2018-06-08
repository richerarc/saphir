# Saphir
[![doc](https://docs.rs/saphir/badge.svg)](https://docs.rs/saphir/)
[![crate](https://img.shields.io/crates/v/saphir.svg)](https://crates.io/crates/saphir)
[![issue](https://img.shields.io/github/issues/richerarc/saphir.svg)](https://github.com/richerarc/saphir/issues)
![downloads](https://img.shields.io/crates/d/saphir.svg)
[![license](https://img.shields.io/crates/l/saphir.svg)](https://github.com/richerarc/saphir/blob/master/LICENSE)

### Saphir is an attempt to a low-level yet not-painful server side rust framework
Rust has plenty of great features, but some of them are causing some pain when getting into the web development game. The goal is to give low-level control to your web stack (as hyper does) without the time consuming task of doing everything from scratch.

## Quick server setup
```rust
#[macro_use]
extern crate saphir;

use saphir::*;

struct TestMiddleware {}

impl Middleware for TestMiddleware {
    fn resolve(&self, req: &SyncRequest, _res: &mut SyncResponse) -> RequestContinuation {
        println!("I'm a middleware");
        println!("{:?}", req);
        RequestContinuation::Next
    }
}

struct TestControllerContext {
    pub resource: String,
}

impl TestControllerContext {
    pub fn new(res: &str) -> Self {
        TestControllerContext {
            resource: res.to_string(),
        }
    }

    pub fn function_to_receive_any_get_http_call(&self, _req: &SyncRequest, res: &mut SyncResponse) {
        res.status(StatusCode::OK).body(format!("this is working nicely!\r\n the context string is : {}", self.resource));
    }
}

fn main() {
    let _ = Server::new()
        .configure_middlewares(|stack| {
            stack.apply(TestMiddleware {}, vec!("/"), None);
        })
        .configure_router(|router| {
            let basic_test_cont = BasicController::new(TestControllerContext::new("this is a private resource"));

            basic_test_cont.add(Method::GET, reg!("/"), TestControllerContext::function_to_receive_any_get_http_call);
            basic_test_cont.add(Method::POST, reg!("/"), |_, _, _| { println!("this was a post request") });
            basic_test_cont.add_with_guards(Method::PUT, "^/patate", BodyGuard.into(), |_,_,_| {println!("this is only reachable if the request has a body")});

            router.add("/", basic_test_cont);
        })
        .configure_listener(|listener_config| {
            listener_config.set_uri("http://0.0.0.0:12345");
        })
        .run();
}
```
