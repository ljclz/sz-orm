# sz-orm-studio

> Web GUI data browser for sz-orm — v5.1.0

## Overview

`sz-orm-studio` provides an axum-based HTTP server for browsing and editing database tables through a web interface. It integrates with sz-orm-core's connection pool and query builder.

## Features

- **Table listing**: `GET /api/tables` — list all tables in the connected database
- **Table data**: `GET /api/tables/{name}/data` — paginated table data with filtering
- **Record editing**: `PUT /api/tables/{name}/records/{id}` — update single record
- **Table relations**: `GET /api/tables/{name}/relations` — foreign key relationship graph

## Quick Start

```rust
use sz_orm_studio::{WebGuiServer, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig {
        bind_addr: "0.0.0.0:8080".to_string(),
        database_url: "mysql://root:test123@127.0.0.1:3306/mydb".to_string(),
        page_size: 50,
    };
    WebGuiServer::run(config).await?;
    Ok(())
}
```

## Architecture

- `server.rs` — `WebGuiServer` + `ServerConfig` + axum router setup
- `handlers.rs` — REST API endpoint handlers (tower-http CORS middleware)
- Dependencies: axum 0.7, tower-http 0.6, parking_lot, serde, serde_json

## Tests

9 integration tests in `tests/studio_e2e.rs` covering server config, route registration, and handler logic.