use std::net::{IpAddr, SocketAddr};

use axum::Extension;

mod users;

pub type Result<T> = anyhow::Result<T>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut port = 8000;
    if let Some(p) = std::env::args().into_iter().collect::<Vec<_>>().get(1) {
        port = p.parse().unwrap();
    }

    let db = sled::Config::new().temporary(true).open()?;
    let users = users::Users::new(&db);

    let app = axum::Router::new().layer(Extension(users));

    let ip = IpAddr::V4([0, 0, 0, 0].into());
    let addr = SocketAddr::new(ip, port);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
