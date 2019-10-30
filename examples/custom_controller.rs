extern crate saphir;

use saphir::*;
use cookie::{Cookie, SameSite};

struct CustomController {
    inner: CustomInner,
}

impl Controller for CustomController {
    fn handle(&self, req: &mut SyncRequest, res: &mut SyncResponse) {
        self.inner.inner_fuction(req, res);
    }

    fn base_path(&self) -> &str {
        "/custom"
    }
}

struct CustomInner {
    pub resource: usize,
}

impl CustomInner {
    pub fn inner_fuction(&self, req: &SyncRequest, res: &mut SyncResponse) {
        // All equest will be handled here
        if let Some(_c) = req.cookies().get("MySuperCookie") {
            res.cookie(Cookie::build("MySecondSuperCookie", "ThisIsAReallyAwesomeValue").http_only(true).path("/custom").same_site(SameSite::Strict).finish());
        } else {
            res.cookie(Cookie::build("MySuperCookie", "ThisIsAnAwesomeValue").http_only(true).path("/custom").same_site(SameSite::Strict).finish());
        }
        res.status(StatusCode::OK);
    }
}

fn main() {
    let server_builder = Server::builder();

    let server = server_builder
        .configure_router(|router| {
            router.add(CustomController { inner: CustomInner { resource: 42 } })
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