use std::{env, net::IpAddr};

use thiserror::Error;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: IpAddr,
    pub port: u16,
    pub searxng_base_url: String,
    pub reader_base_url: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("environment variable {name} has an invalid value: {value}")]
    InvalidValue { name: &'static str, value: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bind: read_env("REFINERY_BIND", "0.0.0.0", |value| value.parse())?,
            port: read_env("REFINERY_PORT", "8080", |value| value.parse())?,
            searxng_base_url: env::var("SEARXNG_BASE_URL")
                .unwrap_or_else(|_| "http://searxng:8888".to_owned()),
            reader_base_url: env::var("READER_BASE_URL")
                .unwrap_or_else(|_| "http://reader:8081".to_owned()),
        })
    }

    pub fn for_test() -> Self {
        Self {
            bind: "127.0.0.1".parse().expect("loopback address is valid"),
            port: 0,
            searxng_base_url: "http://searxng.test".to_owned(),
            reader_base_url: "http://reader.test".to_owned(),
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
