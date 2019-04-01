extern crate saphir;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

use saphir::*;
use parking_lot::RwLock;
use hashbrown::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

static GLOBAL_USER_COUNT: AtomicUsize = ATOMIC_USIZE_INIT;

struct LoggerMiddleware {}

impl Middleware for LoggerMiddleware {
    fn resolve(&self, req: &mut SyncRequest, _res: &mut SyncResponse) -> RequestContinuation {
        println!("{:#?}", req);
        RequestContinuation::Continue
    }
}

#[derive(Serialize, Deserialize)]
struct User {
    first_name: String,
    last_name: String,
}

struct UserControllerContext {
    users: RwLock<HashMap<usize, User>>
}

impl UserControllerContext {
    pub fn new() -> Self {
        UserControllerContext {
            users: RwLock::new(HashMap::new())
        }
    }

    pub fn create(&self, req: &SyncRequest, res: &mut SyncResponse) {
        let json = serde_json::from_slice::<serde_json::Value>(req.body().as_slice());

        match json {
            Ok(body) => {
                let user = User {
                    first_name: body["firstname"].to_string(),
                    last_name: body["lastname"].to_string()
                };

                let user_id = GLOBAL_USER_COUNT.fetch_add(1, Ordering::SeqCst);

                self.users.write().insert(user_id, user);

                res.status(200).body(serde_json::to_vec(&json!({"UserId": user_id})).expect("This is valid json")).header("Content-Type", "application/json");
            }
            Err(_e) => {
                res.status(400);
            }
        }
    }

    pub fn read(&self, req: &SyncRequest, res: &mut SyncResponse) {
        let users = self.users.read();
        if let Some(user) = req.captures().get("user-id").and_then(|user_id_str| user_id_str.parse::<usize>().ok()).and_then(|u_id| users.get(&u_id)) {
            let json = match req.captures().get("claim").as_ref().map(|s| s.as_str()) {
                Some("firstname") => {
                    json! ({
                        "Firstname": &user.first_name
                    })
                }
                Some("lastname") => {
                    json! ({
                        "Lastname": &user.last_name
                    })
                }
                _ => {
                    serde_json::to_value(&user).unwrap_or_else(|_| json!({}))
                }
            };

            res.status(200).body(serde_json::to_vec(&json).expect("This is valid json")).header("Content-Type", "application/json");
        } else {
            res.status(404);
        }
    }

    pub fn update(&self, req: &SyncRequest, res: &mut SyncResponse) {
        let mut users = self.users.write();
        if let Some(user) = req.captures().get("user-id").and_then(|user_id_str| user_id_str.parse::<usize>().ok()).and_then(|u_id| users.get_mut(&u_id)) {
            let json = serde_json::from_slice::<serde_json::Value>(req.body().as_slice());

            match json {
                Ok(body) => {
                    if let Some(f) = body.get("firstname").map(|v| v.to_string()) {
                        user.first_name = f;
                    }

                    if let Some(l) = body.get("lastname").map(|v| v.to_string()) {
                        user.last_name = l;
                    }

                    res.status(200).body(serde_json::to_vec(&user).expect("This is valid json")).header("Content-Type", "application/json");
                }
                Err(_e) => {
                    res.status(400);
                }
            }
        } else {
            res.status(404);
        }
    }

    pub fn delete(&self, req: &SyncRequest, res: &mut SyncResponse) {
        let mut users = self.users.write();
        if let Some(_user) = req.captures().get("user-id").and_then(|user_id_str| user_id_str.parse::<usize>().ok()).and_then(|u_id| users.remove(&u_id)) {
            res.status(StatusCode::OK);
        } else {
            res.status(404);
        }
    }
}

fn main() {
    let server_builder = Server::builder();

    let server = server_builder
        .configure_middlewares(|stack| {
            stack.apply(LoggerMiddleware {}, vec!("/user/<_#r(^[0-9]*$)>"), None)
        })
        .configure_router(|router| {
            let basic_test_cont = BasicController::new("/user", UserControllerContext::new());

            basic_test_cont.add_with_guards(Method::POST, "/", BodyGuard.into(), UserControllerContext::create);

            basic_test_cont.add(Method::GET, "/<user-id>", UserControllerContext::read);

            basic_test_cont.add(Method::GET, "/<user-id>/<claim#r(^(firstname)|(lastname)$)>", UserControllerContext::read);

            basic_test_cont.add(Method::PUT, "/<user-id>", UserControllerContext::update);

            basic_test_cont.add(Method::DELETE, "/<user-id#r(^[0-9]*$)>", UserControllerContext::delete);

            router.add(basic_test_cont)
        })
        .configure_listener(|listener_config| {
            listener_config.set_uri("http://0.0.0.0:12345")
        })
        .build();

    if let Err(e) = server.run() {
        println!("{:?}", e);
        assert!(false);
    }
}