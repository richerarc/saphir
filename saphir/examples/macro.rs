#[macro_use]
extern crate log;

use saphir::prelude::*;
use saphir_macro::controller;

struct UserController {

}

#[controller(base_path="/user", version=1, prefix("api"))]
impl UserController {
    async fn get_user(&self, req: Request) -> (u16, String) {
        (200, "Yo".to_string())
    }
}

async fn test_handler(mut req: Request<Body>) -> (u16, Option<String>) {
    (200, req.captures_mut().remove("variable"))
}

async fn hello_world(_: Request<Body>) -> (u16, &'static str) {
    (200, "Hello, World!")
}

#[tokio::main]
async fn main() -> Result<(), SaphirError> {
    env_logger::init();

    let server = Server::builder()
        .configure_listener(|l| l.interface("127.0.0.1:3000"))
        .configure_router(|r| {
            r.route("/", Method::GET, hello_world)
                .route("/{variable}/print", Method::GET, test_handler)
        })
        .build();

    server.run().await
}
