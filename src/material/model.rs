use serde::{Serialize, Serializer, ser::SerializeStruct};
use url::Url;

use crate::content::ResourceKind;

#[derive(Debug)]
pub struct TargetFacts {
    pub requested_url: Url,
    pub final_url: Url,
    pub resource_kind: ResourceKind,
    pub content_type: String,
}

impl TargetFacts {
    pub fn new(
        requested_url: Url,
        final_url: Url,
        resource_kind: ResourceKind,
        content_type: String,
    ) -> Self {
        Self {
            requested_url,
            final_url,
            resource_kind,
            content_type,
        }
    }
}

impl Serialize for TargetFacts {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("TargetFacts", 4)?;
        state.serialize_field("requested_url", self.requested_url.as_str())?;
        state.serialize_field("final_url", self.final_url.as_str())?;
        state.serialize_field("resource_kind", &self.resource_kind)?;
        state.serialize_field("content_type", &self.content_type)?;
        state.end()
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaterialStatus {
    Extracted,
    Empty,
    Blocked,
    Failed,
    TimedOut,
    DownloadOnly,
}

#[derive(Debug, Serialize)]
pub struct ExtractionResult {
    pub status: MaterialStatus,
    pub engine: String,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reader_content_type: Option<String>,
    pub title: Option<String>,
    pub markdown: String,
    pub links: Vec<crate::content::links::Link>,
}

impl ExtractionResult {
    pub fn blocked(engine: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            status: MaterialStatus::Blocked,
            engine: engine.into(),
            format: "markdown".to_owned(),
            reason: Some(reason.into()),
            reader_content_type: None,
            title: None,
            markdown: String::new(),
            links: Vec::new(),
        }
    }

    pub fn download_only(engine: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            status: MaterialStatus::DownloadOnly,
            engine: engine.into(),
            format: "binary".to_owned(),
            reason: Some(reason.into()),
            reader_content_type: None,
            title: None,
            markdown: String::new(),
            links: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DownloadCapability {
    pub available: bool,
}

#[derive(Debug, Serialize)]
pub struct Diagnostics {
    pub upstream_status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub timeout_seconds: Option<u64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Pagination {
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct MaterialContentResponse {
    pub target: TargetFacts,
    pub extraction: ExtractionResult,
    pub download: DownloadCapability,
    pub pagination: Pagination,
    pub diagnostics: Diagnostics,
}
