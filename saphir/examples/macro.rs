use saphir::prelude::*;
use saphir_macro::controller;

fn guard_string(_controller: &UserController) -> String {
    UserController::BASE_PATH.to_string()
}

async fn print_string_guard(string: &String, req: Request<Body>) -> Result<Request<Body>, &'static str> {
    println!("{}", string);

    Ok(req)
}

struct UserController {

}

#[controller(name="/users", version=1, prefix="api")]
impl UserController {
    #[get("/<user_id>")]
    async fn get_user(&self, req: Request) -> (u16, String) {
        (200, "Yo".to_string())
    }

    #[get("/sync")]
    fn get_user_sync(&self, req: Request<Bytes>) -> (u16, String) {
        (200, "Yo".to_string())
    }

    #[guard(fn="print_string_guard", data="guard_string")]
    #[get("/")]
    async fn list_user(&self, req: Request) -> (u16, String) {
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
