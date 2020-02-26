use saphir::prelude::*;
use saphir_macro::controller;

struct UserController {

}

#[controller(name="/users", version=1, prefix="api")]
impl UserController {
    #[get("/<user_id>")]
    async fn get_user(&self, req: Request) -> (u16, String) {
        (200, "Yo".to_string())
    }

    #[guard(fn="my_fn")]
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
