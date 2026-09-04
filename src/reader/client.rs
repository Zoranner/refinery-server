use axum::http::{HeaderMap, header};
use serde_json::json;
use std::time::Duration;
use url::Url;

use crate::error::ApiError;

const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

pub struct ReaderDocument {
    pub final_url: Url,
    pub content_type: String,
    pub markdown: String,
}

pub async fn read(
    client: &reqwest::Client,
    base_url: &str,
    url: &Url,
    timeout: Duration,
) -> Result<ReaderDocument, ApiError> {
    let endpoint = format!("{}/", base_url.trim_end_matches('/'));
    let response = client
        .post(endpoint)
        .timeout(timeout)
        .header("x-no-cache", "true")
        .header("x-respond-with", "markdown")
        .header("x-retain-links", "all")
        .header("x-timeout", timeout.as_secs().to_string())
        .json(&json!({ "url": url.as_str() }))
        .send()
        .await
        .map_err(map_request_error)?;

    if !response.status().is_success() {
        return Err(ApiError::fetch_failed());
    }

    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(ApiError::response_too_large());
    }

    let headers = response.headers().clone();
    let bytes = read_limited(response).await?;
    let markdown = String::from_utf8(bytes).map_err(|_| ApiError::fetch_failed())?;

    Ok(ReaderDocument {
        final_url: responded_url(&headers).unwrap_or_else(|| url.clone()),
        content_type: headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("text/markdown")
            .to_owned(),
        markdown,
    })
}

async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, ApiError> {
    let mut body = Vec::new();

    while let Some(chunk) = response.chunk().await.map_err(map_request_error)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(ApiError::response_too_large());
        }

        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

fn responded_url(headers: &HeaderMap) -> Option<Url> {
    headers
        .get("x-responded-url")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Url::parse(value).ok())
}

fn map_request_error(error: reqwest::Error) -> ApiError {
    if error.is_timeout() {
        ApiError::fetch_timeout()
    } else {
        ApiError::fetch_failed()
    }
}
