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
    fn resolve(&self, req: &mut SyncRequest, _res: &mut SyncResponse) -> RequestContinuation {
        println!("I'm a middleware");
        println!("{:?}", req);

        let params = if let Some(_query_param_str) = req.uri().query() {
            vec![("param1".to_string(), "value1".to_string()), ("param2".to_string(), "value2".to_string())]
        } else {
            vec![]
        };

        req.addons_mut().add(RequestAddon::new("query_params".to_owned(), params));

        RequestContinuation::Continue
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
    let server = Server::new();

    if let Err(e) = server
        .configure_middlewares(|stack| {
            stack.apply(TestMiddleware {}, vec!("/"), None);
        })
        .configure_router(|router| {
            let basic_test_cont = BasicController::new("^/test", TestControllerContext::new("this is a private resource"));

            basic_test_cont.add(Method::GET, reg!("^/$"), TestControllerContext::function_to_receive_any_get_http_call);

            basic_test_cont.add(Method::POST, reg!("^/$"), |_, _, _| { println!("this was a post request") });

            basic_test_cont.add(Method::GET, reg!("^/query"), |_, req, _| {
                if let Some(query_params) = req.addons().get("query_params") {
                    if let Some(vec_param) = query_params.borrow_as::<Vec<(String, String)>>() {
                        for param in vec_param {
                            println!("{:?}", param);
                        }
                    }
                }
            });

            basic_test_cont.add_with_guards(Method::PUT, "^/patate", BodyGuard.into(), |_,_,_| {println!("this is only reachable if the request has a body")});

            /// This will add the controller and so the following method+route will be valid
            /// GET  /test/
            /// POST /test/
            /// GET  /test/query
            /// PUT  /test/patate
            router.add(basic_test_cont);

            let basic_test_cont2 = BasicController::new("^/test2$", TestControllerContext::new("this is a second private resource"));

            basic_test_cont2.add(Method::GET, reg!("^/$"), |_, _, _| { println!("this was a get request handled by the second controller") });

            /// This will add the controller at the specified route and so the following method+route will be valid
            /// GET  /api/test2/
            router.route("^/api", basic_test_cont2);

        })
        .configure_listener(|listener_config| {
            listener_config.set_uri("http://0.0.0.0:12345");
        })
        .run() {
    }
}
```
