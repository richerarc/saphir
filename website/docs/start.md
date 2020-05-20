---
id: start
title: Introduction
---

Saphir aims to be fast in production but also make it fast for you to write code. It is fully async, boilerplate free, and runs on stable rust.
In this section you are going to learn how to quickly setup a new saphir server. If you're looking to understand each part of a saphir stack, we'd recommend you to take a look at the [Documentation](/docs/doc1)

## Installation
First you will need to add those line to your cargo.toml

```toml title="Cargo.toml"
saphir = { version = "2.6.3", features = ["full"] }
```

:::note Note
We recommend starting with the `full` feature set to facilitate your first experience with the framework
:::
