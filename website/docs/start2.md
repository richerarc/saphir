---
id: start2
title: Basic Server Setup
---

### Configuring the server
Saphir bundles everything you need to start inside the prelude module:

```rust title="src/main.rs"
use saphir::prelude::*;

#[tokio::main]
async fn main() -> Result<(), SaphirError> {
    let server = Server::builder()
        .configure_listener(|listener_builder| {
            listener_builder
              .interface("127.0.0.1:3000")
              .server_name("YourFirstServer")
          }
        ).build();

    server.run().await
}
```
### Adding a request handler

```rust title="src/main.rs"
use saphir::prelude::*;

async fn hello_world(req: Request) -> &'static str {
  "Hello, World!"
}

#[tokio::main]
async fn main() -> Result<(), SaphirError> {
    let server = Server::builder()
        .configure_listener(|listener_builder| {
            listener_builder
              .interface("127.0.0.1:3000")
              .server_name("YourFirstServer")
          }
        )
        .configure_router(|route_builder| {
            route_builder.route("/", Method::GET, hello_world)
        })
        .build();

    server.run().await
}
```
Now, if you make a GET request on http://localhost:3000/ you should see: `Hello, Wolrd`.

### Adding Your First Controller

To define a controller, simply use the `#[controller]` attribute macro on a Rust type `impl` block:

```rust title='pet_controller.rs'

struct PetsController;

#[controller]
impl PetsController {}

```
By adding the attribute macro, you struct `PetController` will automatically derive the controller trait and route its request handlers.
Routing start at a controller's basepath, which is, by default, the name of the Rust type, lowercased with "controller" trimmed.

In our case the basepath is : `/pets`.

To define a controller handler, we need and other attribute, the `#[<method>("<path>")]`. So `#[get("/age")]` will bind the handler to the HTTP Get method on the `/pets/age` path.

Let's say we want an endpoint to get the pets count in our server, we would write:

```rust

struct PetsController;

#[controller]
impl PetsController {
    #[get("/count")]
    async fn get_pets_count(&self) -> (u16, usize) {
        (200, 0) // We have zero pets
    }
}
```
This means that a request at http://localhost:3000/pets/count, will generate a response with status code 200 OK, and a body of `0`.

### Putting it All Together

To add our controller to our server, we will need the `controller` method on the `route_builder` from the `configure_router` method:

```rust
.configure_router(|route_builder| {
    route_builder
        .route("/", Method::GET, hello_world)
        .controller(PetsController)
})
```

Once all is completed, your `main.rs` should look like:

```rust title="src/main.rs"
use saphir::prelude::*;

struct PetsController;

#[controller]
impl PetsController {
    #[get("/count")]
    async fn get_pets_count(&self) -> (u16, usize) {
        (200, 0) // We have zero pets
    }
}

async fn hello_world(req: Request) -> &'static str {
  "Hello, World!"
}

#[tokio::main]
async fn main() -> Result<(), SaphirError> {
    let server = Server::builder()
        .configure_listener(|listener_builder| {
            listener_builder
              .interface("127.0.0.1:3000")
              .server_name("YourFirstServer")
          }
        )
        .configure_router(|route_builder| {
            route_builder
                .route("/", Method::GET, hello_world)
                .controller(PetsController)
        })
        .build();

    server.run().await
}
```

You now have your saphir server up & running! Now you should go see our full documentation to fully understand the powerfull feature set of saphir.