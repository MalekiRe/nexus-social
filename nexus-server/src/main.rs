use std::net::{IpAddr, SocketAddr};

use axum::response::{IntoResponse, Response};
use reqwest::StatusCode;
use sled::transaction::ConflictableTransactionError;

mod users;

pub type Result<T> = std::result::Result<T, AppError>;

pub struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl From<AppError> for ConflictableTransactionError<AppError> {
    fn from(val: AppError) -> Self {
        ConflictableTransactionError::Abort(val)
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut port = 8000;
    if let Some(p) = std::env::args().into_iter().collect::<Vec<_>>().get(1) {
        port = p.parse().unwrap();
    }

    let db = sled::Config::new().temporary(true).open()?;
    let users = users::Users::new(&db);

    // TODO nesting and multiple base routes?
    let app = users.route();

    let ip = IpAddr::V4([0, 0, 0, 0].into());
    let addr = SocketAddr::new(ip, port);
    eprintln!("Hosting on {}...", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
