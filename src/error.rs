use std::borrow::Cow;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: Cow<'static, str>,
    pub stage: &'static str,
    pub retryable: bool,
}

impl ApiError {
    pub fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<Cow<'static, str>>,
        stage: &'static str,
        retryable: bool,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            stage,
            retryable,
        }
    }

    pub fn invalid_request(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            message,
            "request",
            false,
        )
    }

    pub fn upstream_unavailable() -> Self {
        Self::new(
            StatusCode::BAD_GATEWAY,
            "upstream_unavailable",
            "upstream is unavailable",
            "upstream",
            true,
        )
    }

    pub fn upstream_timeout() -> Self {
        Self::new(
            StatusCode::GATEWAY_TIMEOUT,
            "upstream_timeout",
            "upstream timed out",
            "upstream",
            true,
        )
    }

    pub fn blocked_target() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "blocked_target",
            "target address is not allowed",
            "policy",
            false,
        )
    }

    pub fn response_too_large() -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "response_too_large",
            "content response is too large",
            "upstream",
            false,
        )
    }

    pub fn with_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.message = message.into();
        self
    }

    pub fn with_stage(mut self, stage: &'static str) -> Self {
        self.stage = stage;
        self
    }

    pub fn is_protocol_error(&self) -> bool {
        self.code == "invalid_request"
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: Cow<'static, str>,
}
