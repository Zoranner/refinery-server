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
    pub message: &'static str,
}

impl ApiError {
    pub const fn invalid_request(message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message,
        }
    }

    pub const fn search_upstream_failed() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "search_upstream_failed",
            message: "search upstream is unavailable",
        }
    }

    pub const fn blocked_target() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "blocked_target",
            message: "target address is not allowed",
        }
    }

    pub const fn fetch_failed() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "fetch_failed",
            message: "content upstream is unavailable",
        }
    }

    pub const fn fetch_timeout() -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            code: "fetch_timeout",
            message: "content upstream timed out",
        }
    }

    pub const fn response_too_large() -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code: "response_too_large",
            message: "content response is too large",
        }
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
    message: &'static str,
}
