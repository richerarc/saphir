#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::ptr_arg)]
use serde_derive::{Deserialize, Serialize};

use saphir::file::File;
use saphir::prelude::*;

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

struct UserController {}

#[controller(name = "users", version = 1, prefix = "api")]
impl UserController {
    #[get("/<user_id>")]
    async fn get_user(&self, user_id: String, action: Option<u16>) -> (u16, String) {
        (200, format!("user_id: {}, action: {:?}", user_id, action))
    }

    #[post("/json")]
    async fn post_user_json(&self, user: Json<User>) -> (u16, Json<User>) {
        (200, user)
    }

    #[get("/form")]
    #[post("/form")]
    async fn user_form(&self, user: Form<User>) -> (u16, Form<User>) {
        (200, user)
    }

    #[cookies]
    #[post("/sync")]
    fn get_user_sync(&self, mut req: Request<Json<User>>) -> (u16, Json<User>) {
        let u = req.body_mut();
        u.username = "Samuel".to_string();
        (200, Json(u.clone()))
    }

    #[get("/")]
    #[guard(fn = "print_string_guard", data = "guard_string")]
    async fn list_user(&self, _req: Request<Body<Vec<u8>>>) -> (u16, String) {
        (200, "Yo".to_string())
    }

    #[post("/multi")]
    async fn multipart(&self, mul: Multipart) -> (u16, String) {
        let mut multipart_image_count = 0;
        while let Ok(Some(mut f)) = mul.next_field().await {
            if f.content_type() == &mime::IMAGE_PNG {
                let _ = f.save(format!("/tmp/{}.png", f.name())).await;
                multipart_image_count += 1;
            }
        }

        (200, format!("Multipart form data image saved on disk: {}", multipart_image_count))
    }

    #[get("/file")]
    async fn file(&self, _req: Request<Body<Vec<u8>>>) -> (u16, Option<File>) {
        match File::open("/path/to/file").await {
            Ok(file) => (200, Some(file)),
            Err(_) => (500, None),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), SaphirError> {
    env_logger::init();

    let server = Server::builder()
        .configure_listener(|l| l.interface("127.0.0.1:3000").server_name("MacroExample").request_timeout(None))
        .configure_router(|r| r.controller(UserController {}))
        .build();

    server.run().await
}
