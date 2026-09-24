use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
};
use serde::Serialize;

use crate::state::AppState;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(Serialize)]
pub struct ReadyChecks {
    searxng: &'static str,
    reader: &'static str,
    mcp_host: &'static str,
}

#[derive(Serialize)]
pub struct ReadyResponse {
    status: &'static str,
    checks: ReadyChecks,
}

async fn reachable(client: &reqwest::Client, base_url: &str) -> bool {
    let url = format!("{}/", base_url.trim_end_matches('/'));
    match client.get(url).timeout(PROBE_TIMEOUT).send().await {
        Ok(response) => response.status().as_u16() < 500,
        Err(_) => false,
    }
}

fn host_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    if state.config.mcp_allowed_hosts.is_empty() {
        return true;
    }

    let Some(raw) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some((host, port)) = parse_authority(raw) else {
        return false;
    };
    state.config.mcp_allowed_hosts.iter().any(|allowed| {
        parse_authority(allowed).is_some_and(|(allowed_host, allowed_port)| {
            allowed_host == host && (allowed_port.is_none() || allowed_port == port)
        })
    })
}

fn normalize_host(host: &str) -> String {
    host.trim_matches(['[', ']']).to_ascii_lowercase()
}

fn parse_authority(raw: &str) -> Option<(String, Option<u16>)> {
    let authority = axum::http::uri::Authority::try_from(raw.trim()).ok()?;
    Some((normalize_host(authority.host()), authority.port_u16()))
}

pub async fn ready(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<ReadyResponse>) {
    let (searxng, reader) = tokio::join!(
        reachable(&state.http_client, &state.config.searxng_base_url),
        reachable(&state.http_client, &state.config.reader_base_url),
    );
    let checks = ReadyChecks {
        searxng: if searxng { "ok" } else { "unreachable" },
        reader: if reader { "ok" } else { "unreachable" },
        mcp_host: if host_allowed(&state, &headers) {
            "allowed"
        } else {
            "blocked"
        },
    };
    let ready = checks.searxng == "ok" && checks.reader == "ok" && checks.mcp_host == "allowed";
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadyResponse {
            status: if ready { "ready" } else { "degraded" },
            checks,
        }),
    )
}
