pub mod content;
pub mod health;
pub mod openapi;
pub mod rejection;
pub mod resource;
pub mod search;
pub mod sitemap;

use axum::{Router, routing::get};

use crate::{mcp, state::AppState};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .nest_service("/mcp", mcp::http_service(state.clone()))
        .with_state(state)
}
