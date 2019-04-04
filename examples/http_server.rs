extern crate saphir;

use saphir::*;

struct QueryParams(Vec<(String, String)>);

struct TestMiddleware {}

impl Middleware for TestMiddleware {
    fn resolve(&self, request: &mut SyncRequest, _res: &mut SyncResponse) -> RequestContinuation {
        println!("I'm a middleware");
        println!("{:?}", request);

        let params = if let Some(_query_param_str) = request.uri().query() {
            vec![("param1".to_string(), "value1".to_string()), ("param2".to_string(), "value2".to_string())]
        } else {
            vec![]
        };

        request.extensions_mut().insert(QueryParams(params));

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
    let server_builder = Server::builder();

    let server = server_builder
        .configure_middlewares(|stack| {
            stack.apply(TestMiddleware {}, vec!("/"), None)
        })
        .configure_router(|router| {
            let basic_test_cont = BasicController::new("/tester", TestControllerContext::new("this is a private resource"));

            basic_test_cont.add(Method::GET, "/", TestControllerContext::function_to_receive_any_get_http_call);

            basic_test_cont.add(Method::POST, "/", |_, _, _| { println!("this was a post request") });

            basic_test_cont.add(Method::GET, "/panic", |_, _, _| { panic!("lol") });

            basic_test_cont.add(Method::GET, "/timeout", |_, _, _| { std::thread::sleep(std::time::Duration::from_millis(15000)) });

            basic_test_cont.add(Method::GET, "/query", |_, req, _| {
                if let Some(query_params) = req.extensions().get::<QueryParams>() {
                    for param in &query_params.0 {
                        println!("{:?}", param);
                    }
                }
            });

            basic_test_cont.add_with_guards(Method::PUT, "/patate", BodyGuard.into(), |_, _, _| { println!("this is only reachable if the request has a body") });

            let basic_test_cont2 = BasicController::new("/test2", TestControllerContext::new("this is a second private resource"));
            basic_test_cont2.add(Method::GET, "/", |_, _, _| { println!("this was a get request handled by the second controller") });

            // This will add the controller and so the following method+route will be valid
            // GET  /test/
            // POST /test/
            // GET  /test/query
            // PUT  /test/patate

            // This will add the controller at the specified route and so the following method+route will be valid
            // GET  /api/test2/

            router.add(basic_test_cont)
                .route("/test", basic_test_cont2)
        })
        .configure_listener(|listener_config| {
            listener_config.set_uri("http://0.0.0.0:12345")
                .set_request_timeout_ms(10000) // 10 sec
        })
        .build();

    if let Err(e) = server.run() {
        println!("{:?}", e);
        assert!(false);
    }
}