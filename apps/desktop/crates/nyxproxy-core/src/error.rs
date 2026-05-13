//! Error types for the NyxProxy core engine.

use thiserror::Error;

pub type NyxResult<T> = Result<T, NyxError>;

#[derive(Debug, Error)]
pub enum NyxError {
    #[error("certificate authority error: {0}")]
    Ca(String),

    #[error("proxy server error: {0}")]
    Proxy(String),

    #[error("upstream connection error: {0}")]
    Upstream(String),

    #[error("tls error: {0}")]
    Tls(String),

    #[error("http error: {0}")]
    Http(String),

    #[error("decoding error: {0}")]
    Decode(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<rcgen::Error> for NyxError {
    fn from(err: rcgen::Error) -> Self {
        NyxError::Ca(err.to_string())
    }
}

impl From<rustls::Error> for NyxError {
    fn from(err: rustls::Error) -> Self {
        NyxError::Tls(err.to_string())
    }
}

impl From<hyper::Error> for NyxError {
    fn from(err: hyper::Error) -> Self {
        NyxError::Http(err.to_string())
    }
}

impl From<http::Error> for NyxError {
    fn from(err: http::Error) -> Self {
        NyxError::Http(err.to_string())
    }
}

impl From<reqwest::Error> for NyxError {
    fn from(err: reqwest::Error) -> Self {
        NyxError::Upstream(err.to_string())
    }
}
