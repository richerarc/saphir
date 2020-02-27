use saphir::prelude::*;
use saphir_macro::controller;
use serde_derive::{Serialize, Deserialize};

fn guard_string(_controller: &UserController) -> String {
    UserController::BASE_PATH.to_string()
}

async fn print_string_guard(string: &String, req: Request<Body>) -> Result<Request<Body>, &'static str> {
    println!("{}", string);

    Ok(req)
}

#[derive(Serialize, Deserialize, Clone)]
struct User {
    username: String,
    age: i64,
}

struct UserController {

}

#[controller(name="/users", version=1, prefix="api")]
impl UserController {
    #[get("/<user_id>")]
    async fn get_user(&self, req: Request) -> (u16, String) {
        (200, "Yo".to_string())
    }

    #[post("/sync")]
    fn get_user_sync(&self, mut req: Request<Json<User>>) -> (u16, Json<User>) {
        let mut u = req.body_mut();
        u.username = "Samuel".to_string();
        (200, Json(u.clone()))
    }

    #[guard(fn="print_string_guard", data="guard_string")]
    #[get("/")]
    async fn list_user(&self, req: Request<Body<Vec<u8>>>) -> (u16, String) {
        (200, "Yo".to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), SaphirError> {
    env_logger::init();

    let server = Server::builder()
        .configure_listener(|l| l.interface("127.0.0.1:3000"))
        .configure_router(|r| {
            r.controller(UserController {})
        })
        .build();

    server.run().await
}
