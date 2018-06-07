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

    pub fn function_to_receive_any_get_http_call(&self, req: &SyncRequest, res: &mut SyncResponse) {
        use saphir::header::*;
        res.status(StatusCode::OK).body(format!("this is working nicely!\r\n the context string is : {}", self.resource));
    }
}

#[test]
fn simple_http_server() {
    let mut mid_stack = MiddlewareStack::new();

    mid_stack.apply(TestMiddleware {}, vec!("/"), None);

    let basic_test_cont = BasicController::new(TestControllerContext::new("this is a private resource"));

    basic_test_cont.add(Method::GET, reg!("/"), TestControllerContext::function_to_receive_any_get_http_call);
    basic_test_cont.add(Method::POST, reg!("/"), |_, _, _| { println!("this was a post request") });
    basic_test_cont.add_with_guards(Method::PUT, "^/patate", BodyGuard.into(), |_,_,_| {println!("this is only reachable if the request has a body")});

    let mut router = Router::new();

    router.add("/", basic_test_cont);

    let server = Server::new(router, Some(mid_stack));

    let _ = server.run(12345);
}