# Saphir
[![Saphir doc badge](https://docs.rs/saphir/badge.svg)](https://docs.rs/saphir/)
[![Saphir crate badge](https://img.shields.io/crates/v/saphir.svg)](https://img.shields.io/crates/v/saphir.svg)
[![Saphir downloads badge](https://img.shields.io/crates/d/saphir.svg)](https://img.shields.io/crates/d/saphir.svg)
[![Saphir license badge](https://img.shields.io/crates/l/saphir.svg)](https://img.shields.io/crates/l/saphir.svg)
[![Saphir issue badge](https://img.shields.io/github/issues/richerarc/saphir.svg)](https://img.shields.io/github/issues/richerarc/saphir.svg)

## Quick server setup
```rust
#[macro_use]
extern crate saphir;
extern crate regex;

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

impl Default for TestControllerContext {
    fn default() -> Self {
        TestControllerContext {
            resource: "This is a string".to_string(),
        }
    }
}

fn function_to_receive_any_get_http_call(context: &TestControllerContext, _req: &SyncRequest, res: &mut SyncResponse) {
    res.status(StatusCode::OK).body(format!("this is working nicely!\r\n the context string is : {}", context.resource));
}

fn main() {
    let mut mid_stack = MiddlewareStack::new();

    mid_stack.apply(TestMiddleware {}, vec!("/"), None);

    let basic_test_cont = BasicController::new(TestControllerContext::default());

    basic_test_cont.add(Method::GET, reg!("/"), function_to_receive_any_get_http_call);
    basic_test_cont.add(Method::POST, reg!("/"), |_, _, _| { println!("this was a post request") });
    basic_test_cont.add_with_guards(Method::PUT, "^/patate", BodyGuard.into(), |_,_,_| {println!("this is only reachable if the request has a body")});

    let mut router = Router::new();

    router.add("/", basic_test_cont);

    let server = Server::new(router, Some(mid_stack));

    let _ = server.run(12345);
}
```