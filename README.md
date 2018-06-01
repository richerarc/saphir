# Saphir
[![Saphir doc badge](https://docs.rs/saphir/badge.svg)](https://docs.rs/saphir/)

## Quick server setup
```rust
#[macro_use]
extern crate saphir;
extern crate regex;

use saphir::*;

struct TestMiddleware {

}

impl Middleware for TestMiddleware {
    fn resolve(&self, req: &SyncRequest, _res: &mut Response<Body>) -> RequestContinuation {
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

fn function_to_receive_any_get_http_call(context: &TestControllerContext, _req: &SyncRequest, _res: &mut Response) {
    println!("This is from controller 1");
    println!("{}", context.resource);
}

fn main() {
    let mut mid_stack = MiddlewareStack::new();

    mid_stack.apply(TestMiddleware{}, vec!("/"), None);

    let basic_test_cont = BasicController::new(TestControllerContext::default());

    basic_test_cont.add(Method::Get, reg!("/"), function_to_receive_any_get_http_call);
    basic_test_cont.add(Method::Post, reg!("/"), |_, _, _| {println!("this was a post request")});

    let mut router = Router::new();

    router.add("/", basic_test_cont);

    let server = Server::new(router, Some(mid_stack));

    let _ = server.run(12345);
}
```