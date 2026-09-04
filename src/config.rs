use std::{env, net::IpAddr, time::Duration};

use thiserror::Error;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: IpAddr,
    pub port: u16,
    pub searxng_base_url: String,
    pub reader_base_url: String,
    pub resource_timeout: Duration,
    pub reader_timeout: Duration,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("environment variable {name} has an invalid value: {value}")]
    InvalidValue { name: &'static str, value: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bind: read_env("HTTP_LISTEN_ADDRESS", "0.0.0.0", |value| value.parse())?,
            port: read_env("HTTP_LISTEN_PORT", "8080", |value| value.parse())?,
            searxng_base_url: env::var("SEARXNG_BASE_URL")
                .unwrap_or_else(|_| "http://searxng:8888".to_owned()),
            reader_base_url: env::var("READER_BASE_URL")
                .unwrap_or_else(|_| "http://reader:8081".to_owned()),
            resource_timeout: read_duration_env("RESOURCE_REQUEST_TIMEOUT_SECONDS", "20")?,
            reader_timeout: read_duration_env("READER_REQUEST_TIMEOUT_SECONDS", "60")?,
        })
    }

    pub fn for_test() -> Self {
        Self {
            bind: "127.0.0.1".parse().expect("loopback address is valid"),
            port: 0,
            searxng_base_url: "http://searxng.test".to_owned(),
            reader_base_url: "http://reader.test".to_owned(),
            resource_timeout: Duration::from_secs(20),
            reader_timeout: Duration::from_secs(60),
        }
    }
}

fn read_env<T>(
    name: &'static str,
    default: &'static str,
    parse: impl FnOnce(&str) -> Result<T, <T as std::str::FromStr>::Err>,
) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    let value = env::var(name).unwrap_or_else(|_| default.to_owned());
    parse(&value).map_err(|_| ConfigError::InvalidValue { name, value })
}

fn read_duration_env(name: &'static str, default: &'static str) -> Result<Duration, ConfigError> {
    let value = env::var(name).unwrap_or_else(|_| default.to_owned());
    value
        .parse::<u64>()
        .map(Duration::from_secs)
        .map_err(|_| ConfigError::InvalidValue { name, value })
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn reads_explicit_environment_configuration() {
        unsafe {
            std::env::set_var("HTTP_LISTEN_ADDRESS", "127.0.0.2");
            std::env::set_var("HTTP_LISTEN_PORT", "9090");
            std::env::set_var("SEARXNG_BASE_URL", "http://search.test:8888");
            std::env::set_var("READER_BASE_URL", "http://reader.test:8081");
            std::env::set_var("RESOURCE_REQUEST_TIMEOUT_SECONDS", "7");
            std::env::set_var("READER_REQUEST_TIMEOUT_SECONDS", "60");
        }

        let config = Config::from_env().expect("explicit environment is valid");

        assert_eq!(config.bind.to_string(), "127.0.0.2");
        assert_eq!(config.port, 9090);
        assert_eq!(config.searxng_base_url, "http://search.test:8888");
        assert_eq!(config.reader_base_url, "http://reader.test:8081");
        assert_eq!(config.resource_timeout, std::time::Duration::from_secs(7));
        assert_eq!(config.reader_timeout, std::time::Duration::from_secs(60));

        unsafe {
            std::env::remove_var("HTTP_LISTEN_ADDRESS");
            std::env::remove_var("HTTP_LISTEN_PORT");
            std::env::remove_var("SEARXNG_BASE_URL");
            std::env::remove_var("READER_BASE_URL");
            std::env::remove_var("RESOURCE_REQUEST_TIMEOUT_SECONDS");
            std::env::remove_var("READER_REQUEST_TIMEOUT_SECONDS");
        }
    }
}
