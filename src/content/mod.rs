pub mod chunk;
pub mod links;

use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::error::ApiError;

#[derive(Debug, Deserialize)]
pub struct ContentRequest {
    pub url: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_max_chars")]
    pub max_chars: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Html,
    Text,
    Pdf,
    Image,
    Unknown,
}
impl ContentRequest {
    pub fn validate(&self) -> Result<Url, ApiError> {
        if !(1000..=24000).contains(&self.max_chars) {
            return Err(ApiError::invalid_request(
                "max_chars must be between 1000 and 24000",
            ));
        }

        validate_public_url(&self.url)
    }
}

fn default_max_chars() -> usize {
    12000
}

pub fn kind_for_url(url: &Url) -> ResourceKind {
    let path = url.path().to_ascii_lowercase();

    if path.ends_with(".pdf") {
        ResourceKind::Pdf
    } else if [".txt", ".md", ".csv", ".json", ".xml", ".yaml", ".yml"]
        .iter()
        .any(|extension| path.ends_with(extension))
    {
        ResourceKind::Text
    } else if [
        ".avif", ".bmp", ".gif", ".ico", ".jpeg", ".jpg", ".png", ".svg", ".tif", ".tiff", ".webp",
    ]
    .iter()
    .any(|extension| path.ends_with(extension))
    {
        ResourceKind::Image
    } else if [".asp", ".aspx", ".htm", ".html", ".jsp", ".php", ".xhtml"]
        .iter()
        .any(|extension| path.ends_with(extension))
    {
        ResourceKind::Html
    } else if path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'))
    {
        ResourceKind::Unknown
    } else {
        ResourceKind::Html
    }
}

pub fn kind_for_response(url: &Url, content_type: &str) -> ResourceKind {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    match mime.as_str() {
        "application/pdf" => ResourceKind::Pdf,
        value if value.starts_with("image/") => ResourceKind::Image,
        "text/html" => ResourceKind::Html,
        "application/json" | "application/xml" | "application/yaml" | "text/csv" | "text/xml"
        | "text/yaml" => ResourceKind::Text,
        "text/plain" => match kind_for_url(url) {
            ResourceKind::Text | ResourceKind::Unknown => ResourceKind::Text,
            kind => kind,
        },
        _ => kind_for_url(url),
    }
}

pub fn validate_public_url(raw: &str) -> Result<Url, ApiError> {
    let url = Url::parse(raw).map_err(|_| ApiError::invalid_request("url is invalid"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::blocked_target());
    }

    let Some(host) = url.host() else {
        return Err(ApiError::invalid_request("url must include a host"));
    };

    if matches!(host, Host::Ipv4(address) if !is_public_ipv4(address))
        || matches!(host, Host::Ipv6(address) if !is_public_ipv6(address))
    {
        return Err(ApiError::blocked_target());
    }

    Ok(url)
}

pub fn is_public_ipv4(address: std::net::Ipv4Addr) -> bool {
    let [first, second, third, _] = address.octets();

    !matches!(first, 0 | 10 | 127 | 224..=255)
        && !(first == 100 && (64..=127).contains(&second))
        && !(first == 169 && second == 254)
        && !(first == 172 && (16..=31).contains(&second))
        && !(first == 192 && second == 168)
        && !(first == 192 && second == 0)
        && !(first == 192 && second == 0 && third == 2)
        && !(first == 198 && (second == 18 || second == 19))
        && !(first == 198 && second == 51 && third == 100)
        && !(first == 203 && second == 0 && third == 113)
}

pub fn is_public_ipv6(address: std::net::Ipv6Addr) -> bool {
    let segments = address.segments();

    !address.is_loopback()
        && !address.is_unspecified()
        && !address.is_multicast()
        && (segments[0] & 0xffc0) != 0xfe80
        && (segments[0] & 0xfe00) != 0xfc00
        && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
}
