pub mod content;
pub mod health;
pub mod openapi;
pub mod resource;
pub mod search;
pub mod sitemap;

use axum::{
    Router,
    routing::{get, post},
};

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/openapi.json", get(openapi::document))
        .route("/v1/content", post(content::content))
        .route("/v1/resource", get(resource::resource))
        .route("/v1/search", post(search::search))
        .route("/v1/sitemap", post(sitemap::sitemap))
        .with_state(state)
}
