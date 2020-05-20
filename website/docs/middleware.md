---
id: middleware
title: Middleware
---

Middlewares process incoming http request as well as outgoing http response. Each middleware will at some point either call the following one, or produce a response and stop request processing.
In the following example we are going to implement a `LogMiddleware`. This middleware task is to log information about request results.
## Middleware definition

```rust  title="log_middleware.rs"
    use saphir::prelude::*;

    pub struct LogMiddleware;

    #[middleware]
    impl LogMiddleware {
        async fn next(&self, ctx: HttpContext, chain: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError> {
            ...
        }
    }
```

There is a few things going on here. 
### The `#[middleware]` proc_macro attribute.
The macro executed by this attribute will look in your implementation and find the function called `next`. _This will be the middleware_.
It requires two things: It needs to be on top of an `impl block` and the `next` function needs the signature above. if those two condition are meet, you're good to go.

### The `HttpContext`
`HttpContext` is a type representing the state of a request going through the server. There is two possible state for the context, `Before` & `After`, which are respectively `Before` the request is processed by the resolved handler, and `After` being processed.

### The Middleware Chain
The middleware chain is a reference stack, containing the next element (middleware) or eventually chaining the controller handler. Calling `chain.next(ctx)` will continue the request processing and automatically trasition the `HttpContext` state to `After` before returning.

### The Async Part
It was obvious but it is written here to, all processing is **async**, so you'll need to explicitly `.await` your stuff.

## Accessing the Request / Response
There is two main way of accessing the request from the context: using `ctx.state.request()` or `ctx.state.request_unchecked()`. The main difference is that `request_unchecked` will `panic!` if called `After` the context state was trasitioned, wereas `request` whould return `None`. The same pattern applies for `ctx.state.response()` vs `ctx.state.response_unchecked()`.
```rust  title="log_middleware.rs"
...

async fn next(&self, ctx: HttpContext, chain: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError> {
    let request = ctx.state.request_unchecked();
    let uri = request.uri().path().to_string();
    let method = request.method().as_str().to_string();

    let ctx = chain.next(ctx).await?; // Trasition state to `After`

    let status = ctx.state.response_unchecked().status().to_string();

    println!(
        "{} {} {}",
        method,
        uri,
        status,
    );
}
```

## Transitionnig the Context Manually

This will happens whenever you will want your middleware to terminate to request processing, in that case you won't call `chain.next(ctx)` since it would mean the call the next middleware. In that case you will use the `ctx.after()` method to transition the context state yourself.

Take this very usefull _OnlyAllowGetRequestMiddleware_ as an example. As shown by its name, the goal is to only allow GET http request.

```rust  title="only_get_middleware.rs"
...

async fn next(&self, ctx: HttpContext, chain: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError> {
    if let Method::GET = ctx.state.request_unchecked().method() {
        chain.next(ctx).await
    } else {
        ctx.after(Builder::new().status(451).build()?);
        Ok(ctx)
    }
}
```
