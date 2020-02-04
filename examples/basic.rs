#[macro_use]
extern crate log;

use futures::future::Ready;
use tokio::sync::RwLock;
use saphir::prelude::*;

// == controller == //

struct MagicController {
    label: String,
}

impl MagicController {
    pub fn new<S: Into<String>>(label: S) -> Self {
        Self { label: label.into() }
    }
}

impl Controller for MagicController {
    const BASE_PATH: &'static str = "/magic";

    fn handlers(&self) -> Vec<ControllerEndpoint<Self>>
    where
        Self: Sized,
    {
        EndpointsBuilder::new()
            .add(Method::GET, "/{delay}", MagicController::magic_delay)
            .add(Method::GET, "/", magic_handler)
            .add(Method::POST, "/", MagicController::read_body)
            .build()
    }
}

impl MagicController {
    async fn magic_delay(&self, req: Request<Body>) -> (u16, String) {
        if let Some(delay) = req.captures().get("delay").and_then(|t| t.parse::<u64>().ok()) {
            tokio::time::delay_for(tokio::time::Duration::from_secs(delay)).await;
            (200, format!("Delayed of {} secs: {}", delay, self.label))
        } else {
            (400, "Invalid timeout".to_owned())
        }
    }

    async fn read_body(&self, req: Request<Body>) -> (u16, Vec<u8>) {
        let req = req.async_map(|b| async {hyper::body::to_bytes(b).await}).await.transpose().unwrap();

        if let Ok(b) = req.body() {
            (200, b.to_vec())
        } else {
            (400, Vec::new())
        }
    }
}

fn magic_handler(controller: &MagicController, _: Request<Body>) -> Ready<(u16, String)> {
    futures::future::ready((200, controller.label.clone()))
}

// == middleware == //

struct StatsData {
    entered: RwLock<u32>,
    exited: RwLock<u32>,
}

impl StatsData {
    fn new() -> Self {
        Self {
            entered: RwLock::new(0),
            exited: RwLock::new(0),
        }
    }

    async fn stats_middleware(
        &self,
        ctx: HttpContext<Body>,
        chain: &dyn MiddlewareChain,
    ) -> Result<Response<Body>, SaphirError> {
        {
            let mut entered = self.entered.write().await;
            let exited = self.exited.read().await;

            *entered += 1;

            info!(
                "entered stats middleware! Current data: entered={} ; exited={}",
                *entered, *exited
            );
        }

        let res = chain.next(ctx).await?;

        {
            let mut exited = self.exited.write().await;
            let entered = self.entered.read().await;

            *exited += 1;

            info!(
                "exited stats middleware! Current data: entered={} ; exited={}",
                *entered, *exited
            );
        }

        Ok(res)
    }
}

async fn log_middleware(
    prefix: &String,
    ctx: HttpContext<Body>,
    chain: &dyn MiddlewareChain,
) -> Result<Response<Body>, SaphirError> {
    info!("{} | new request on path: {}", prefix, ctx.request.uri().path());
    let res = chain.next(ctx).await?;
    info!("{} | new response with status: {}", prefix, res.status());
    Ok(res)
}

// == handlers with no controller == //

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
                .controller(MagicController::new("Just Like Magic!"))
        })
        .configure_middlewares(|m| {
            m.apply(log_middleware, "LOG".to_string(), vec!["/"], None)
                .apply(StatsData::stats_middleware, StatsData::new(), vec!["/"], None)
        })
        .build();

    server.run().await
}
