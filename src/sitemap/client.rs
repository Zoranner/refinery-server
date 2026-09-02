use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, redirect::Policy};
use url::Url;

use crate::{
    content::{is_public_ipv4, is_public_ipv6, validate_public_url},
    error::ApiError,
    sitemap::SitemapFetcher,
};

const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

pub struct HttpSitemapFetcher {
    client: Client,
}

impl HttpSitemapFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("sitemap client configuration is valid");

        Self { client }
    }
}

impl Default for HttpSitemapFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SitemapFetcher for HttpSitemapFetcher {
    async fn get(&self, url: &Url) -> Result<String, ApiError> {
        let mut current = validate_public_url(url.as_str())?;

        for redirects in 0..=MAX_REDIRECTS {
            ensure_public_dns(&current).await?;
            let response = self
                .client
                .get(current.clone())
                .send()
                .await
                .map_err(map_error)?;

            if response.status().is_redirection() {
                if redirects == MAX_REDIRECTS {
                    return Err(ApiError::fetch_failed());
                }

                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(ApiError::fetch_failed)?;
                current = validate_public_url(
                    current
                        .join(location)
                        .map_err(|_| ApiError::fetch_failed())?
                        .as_str(),
                )?;
                continue;
            }

            if !response.status().is_success() {
                return Err(ApiError::fetch_failed());
            }

            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
            {
                return Err(ApiError::response_too_large());
            }

            let bytes = read_limited(response).await?;
            return String::from_utf8(bytes).map_err(|_| ApiError::fetch_failed());
        }

        Err(ApiError::fetch_failed())
    }
}

pub async fn ensure_public_dns(url: &Url) -> Result<(), ApiError> {
    let host = url.host_str().ok_or_else(ApiError::fetch_failed)?;
    let port = url
        .port_or_known_default()
        .ok_or_else(ApiError::fetch_failed)?;
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ApiError::fetch_failed())?;
    let mut found = false;

    for address in addresses {
        found = true;
        if !match address.ip() {
            std::net::IpAddr::V4(address) => is_public_ipv4(address),
            std::net::IpAddr::V6(address) => is_public_ipv6(address),
        } {
            return Err(ApiError::blocked_target());
        }
    }

    found.then_some(()).ok_or_else(ApiError::fetch_failed)
}

async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, ApiError> {
    let mut body = Vec::new();

    while let Some(chunk) = response.chunk().await.map_err(map_error)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(ApiError::response_too_large());
        }

        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

fn map_error(error: reqwest::Error) -> ApiError {
    if error.is_timeout() {
        ApiError::fetch_timeout()
    } else {
        ApiError::fetch_failed()
    }
}
