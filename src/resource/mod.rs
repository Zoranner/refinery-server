use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, redirect::Policy};
use url::Url;

use crate::{
    content::validate_public_url,
    error::ApiError,
    material::download::{DownloadError, DownloadStage, within_budget},
    sitemap::client::ensure_public_dns,
};

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

    async fn get_with_timeout(&self, url: &Url, timeout: Duration) -> Result<Resource, ApiError> {
        tokio::time::timeout(timeout, self.get(url))
            .await
            .map_err(|_| ApiError::fetch_timeout())?
    }
}

pub struct HttpResourceFetcher {
    client: Client,
    timeout: Duration,
}

impl HttpResourceFetcher {
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(20))
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(timeout)
            .build()
            .expect("resource client configuration is valid");

        Self { client, timeout }
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
        self.get_diagnosed(url)
            .await
            .map_err(DownloadError::into_api_error)
    }
}

impl HttpResourceFetcher {
    async fn get_diagnosed(&self, url: &Url) -> Result<Resource, DownloadError> {
        let mut current = validate_public_url(url.as_str())
            .map_err(|error| DownloadError::new(DownloadStage::Connect, error))?;

        for redirects in 0..=MAX_REDIRECTS {
            within_budget(
                async {
                    ensure_public_dns(&current)
                        .await
                        .map_err(|error| DownloadError::new(DownloadStage::Connect, error))
                },
                self.timeout,
                DownloadStage::Connect,
            )
            .await?;
            let response = self
                .client
                .get(current.clone())
                .send()
                .await
                .map_err(|error| DownloadError::new(DownloadStage::Connect, map_error(error)))?;

            if response.status().is_redirection() {
                if redirects == MAX_REDIRECTS {
                    return Err(DownloadError::new(
                        DownloadStage::Redirect,
                        ApiError::fetch_failed(),
                    ));
                }

                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| {
                        DownloadError::new(DownloadStage::Redirect, ApiError::fetch_failed())
                    })?;
                current = validate_public_url(
                    current
                        .join(location)
                        .map_err(|_| {
                            DownloadError::new(DownloadStage::Redirect, ApiError::fetch_failed())
                        })?
                        .as_str(),
                )
                .map_err(|error| DownloadError::new(DownloadStage::Redirect, error))?;
                continue;
            }

            if !response.status().is_success() {
                return Err(DownloadError::new(
                    DownloadStage::Connect,
                    ApiError::fetch_failed(),
                ));
            }

            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();

            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESOURCE_BYTES as u64)
            {
                return Err(DownloadError::new(
                    DownloadStage::Body,
                    ApiError::response_too_large(),
                ));
            }

            let bytes = read_limited(response)
                .await
                .map_err(|error| DownloadError::new(DownloadStage::Body, error))?;
            return Ok(Resource {
                final_url: current,
                content_type,
                bytes,
            });
        }

        Err(DownloadError::new(
            DownloadStage::Redirect,
            ApiError::fetch_failed(),
        ))
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
