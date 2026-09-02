use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, redirect::Policy};
use url::Url;

use crate::{content::validate_public_url, error::ApiError, sitemap::client::ensure_public_dns};

const MAX_RESOURCE_BYTES: usize = 20 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

pub struct Resource {
    pub final_url: Url,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[async_trait]
pub trait ResourceFetcher: Send + Sync {
    async fn get(&self, url: &Url) -> Result<Resource, ApiError>;
}

pub struct HttpResourceFetcher {
    client: Client,
}

impl HttpResourceFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("resource client configuration is valid");

        Self { client }
    }
}

impl Default for HttpResourceFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ResourceFetcher for HttpResourceFetcher {
    async fn get(&self, url: &Url) -> Result<Resource, ApiError> {
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

            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();

            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESOURCE_BYTES as u64)
            {
                return Err(ApiError::response_too_large());
            }

            let bytes = read_limited(response).await?;
            return Ok(Resource {
                final_url: current,
                content_type,
                bytes,
            });
        }

        Err(ApiError::fetch_failed())
    }
}

async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, ApiError> {
    let mut body = Vec::new();

    while let Some(chunk) = response.chunk().await.map_err(map_error)? {
        if body.len() + chunk.len() > MAX_RESOURCE_BYTES {
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
