---
id: start
title: Introduction
---

Saphir aims to be fast in production but also make it fast for you to write code. It is fully async, boilerplate free, and runs on stable rust.
In this section you are going to learn how to quickly setup a new saphir server. If you're looking to understand each part of a saphir stack, we'd recommend you to take a look at the [Documentation](/docs/doc1)

### Installation
First you will need to add those line to your cargo.toml

```toml title="Cargo.toml"
saphir = "2.6.3"
tokio = { version = "0.2.13", features = ["full"] }
```

Saphir runtime is tied to tokio, therefore tokio is needed to bootstrap a new server

:::note Note
We strongly recommend using the `full` cargo feature for tokio.
:::
