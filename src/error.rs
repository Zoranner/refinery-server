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
    pub resource_url: Option<String>,
}

impl ApiError {
    pub const fn invalid_request(message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message,
            resource_url: None,
        }
    }

    pub const fn search_upstream_failed() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "search_upstream_failed",
            message: "search upstream is unavailable",
            resource_url: None,
        }
    }

    pub const fn blocked_target() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "blocked_target",
            message: "target address is not allowed",
            resource_url: None,
        }
    }

    pub const fn fetch_failed() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "fetch_failed",
            message: "content upstream is unavailable",
            resource_url: None,
        }
    }

    pub const fn fetch_timeout() -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            code: "fetch_timeout",
            message: "content upstream timed out",
            resource_url: None,
        }
    }

    pub const fn response_too_large() -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code: "response_too_large",
            message: "content response is too large",
            resource_url: None,
        }
    }

    pub const fn unsupported_media_type() -> Self {
        Self {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            code: "unsupported_media_type",
            message: "resource media type is not supported",
            resource_url: None,
        }
    }

    pub fn resource_download_required(url: &url::Url) -> Self {
        let encoded: String =
            url::form_urlencoded::byte_serialize(url.as_str().as_bytes()).collect();

        Self {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            code: "resource_download_required",
            message: "该资源不支持文本抽取，请通过资源下载接口获取原文件",
            resource_url: Some(format!("/v1/resource?url={encoded}")),
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
                resource_url: self.resource_url,
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorBody,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_url: Option<String>,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}
