use std::net::SocketAddr;

use refinery::{config::Config, routes, state::AppState};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    let address = SocketAddr::from((config.bind, config.port));
    let listener = tokio::net::TcpListener::bind(address).await?;

    info!(address = %listener.local_addr()?, "refinery is listening");
    axum::serve(listener, routes::router(AppState::new(config))).await?;

    Ok(())
}
