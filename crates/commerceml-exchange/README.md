# commerceml-exchange

Server side of the **1C:Enterprise ↔ website exchange protocol**
(`/1c_exchange.php?type=catalog&mode=checkauth`, `init`, `file`, `import`,
`query`, `success`, …) built on [`commerceml`](https://crates.io/crates/commerceml).

You implement `ExchangeHandler` (authentication and what to do with the
data); the crate handles sessions, `init` parameters, uploads in parts, zip
archives, `progress` replies and size limits, and guards against path
traversal, zip-slip and zip bombs. The protocol core is framework
independent; the default `axum` feature adds a ready-made router.

```rust,no_run
use commerceml_exchange::{Credentials, Exchange, ExchangeConfig, ExchangeHandler, FsStorage};

struct Shop(Credentials);

impl ExchangeHandler for Shop {
    async fn authenticate(&self, login: &str, password: &str) -> bool {
        self.0.check(login, password)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let exchange = Exchange::new(
        Shop(Credentials::new("1c", "secret")),
        FsStorage::new("exchange-data")?,
        ExchangeConfig::default(),
    );
    let app = commerceml_exchange::axum::router_at("/1c_exchange.php", exchange);
    axum::serve(tokio::net::TcpListener::bind("0.0.0.0:8080").await?, app).await?;
    Ok(())
}
```

See [`examples/server.rs`](https://github.com/safdafang/commerceml-rs/blob/main/crates/commerceml-exchange/examples/server.rs)
for a complete demo.

License: MIT OR Apache-2.0.
