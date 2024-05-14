use log::info;
use saphir::{file::middleware::FileMiddlewareBuilder, prelude::*};
use serde_derive::{Deserialize, Serialize};

struct PrintGuard {
    inner: String,
}

#[guard]
impl PrintGuard {
    pub fn new(inner: &str) -> Self {
        PrintGuard { inner: inner.to_string() }
    }

    async fn validate(&self, req: Request) -> Result<Request, u16> {
        info!("{}", self.inner);
        Ok(req)
    }
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
    #[validator(exclude("user"))]
    async fn post_user_json(&self, user: Json<User>) -> (u16, Json<User>) {
        (200, user)
    }

    #[get("/form")]
    #[post("/form")]
    #[validator(exclude("user"))]
    async fn user_form(&self, user: Form<User>) -> (u16, Form<User>) {
        (200, user)
    }

    #[cookies]
    #[post("/sync")]
    #[validator(exclude("req"))]
    fn get_user_sync(&self, mut req: Request<Json<User>>) -> (u16, Json<User>) {
        let u = req.body_mut();
        u.username = "Samuel".to_string();
        (200, Json(u.clone()))
    }

    #[get("/")]
    #[guard(PrintGuard, init_expr = "UserController::BASE_PATH")]
    async fn list_user(&self, _req: Request<Body>) -> (u16, String) {
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

#[allow(dead_code)]
struct ApiKeyMiddleware(String);

#[middleware]
impl ApiKeyMiddleware {
    pub fn new(api_key: &str) -> Self {
        ApiKeyMiddleware(api_key.to_string())
    }

    async fn next(&self, ctx: HttpContext, chain: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError> {
        if let Some(Ok("Bearer secure-key")) = ctx
            .state
            .request_unchecked()
            .headers()
            .get(header::AUTHORIZATION)
            .map(|auth_value| auth_value.to_str())
        {
            info!("Authenticated");
        } else {
            info!("Not Authenticated");
        }

        info!("Handler {} will be used", ctx.metadata.name.unwrap_or("unknown"));
        chain.next(ctx).await
    }
}

#[tokio::main]
async fn main() -> Result<(), SaphirError> {
    env_logger::init();

    let file_middleware = FileMiddlewareBuilder::new("op", "./saphir/examples/files_to_serve").build()?;
    let server = Server::builder()
        .configure_listener(|l| l.interface("127.0.0.1:3000").server_name("MacroExample").request_timeout(None))
        .configure_middlewares(|m| {
            m.apply(ApiKeyMiddleware::new("secure-key"), vec!["/"], None)
                .apply(file_middleware, vec!["/op/"], None)
        })
        .configure_router(|r| r.controller(UserController {}))
        .build();

    server.run().await
}
