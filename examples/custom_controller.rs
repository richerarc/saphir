extern crate saphir;

use saphir::*;

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
    pub fn inner_fuction(&self, _req: &SyncRequest, res: &mut SyncResponse) {
        // All equest will be handled here
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