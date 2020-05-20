---
id: stack
title: Saphir's Stack
---

Saphir's stack is composed of a couple of key elements.
Each element of the stack serve its own purpose, then passes the request to the next, until the request is fully handled (or stopped).

![Saphir stack](/static/img/stack.svg)

To be more precise, the request enters the server where the routers determine if a path exists for the current request path. 

Once the route is determined, the request passes through the middleware stack, each middleware can either call the next one or stop the request processing in place. 

After all middleware are called, the router will dispatch the request through appropriate request guard, then finnally to the appropriate controller handler.

Once the handler returns, the returned object (Responder) will generate a response. 

The response will then go through the middleware stack in reverse.