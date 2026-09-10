use crate::{
    content::{chunk, links},
    material::{
        Diagnostics, DownloadCapability, ExtractionResult, MaterialContentResponse, MaterialStatus,
        Pagination, TargetFacts,
    },
};

const DEFAULT_MAX_CHARS: usize = 12000;

pub fn normalize_reader_result(
    target: TargetFacts,
    markdown: String,
    reader_content_type: String,
) -> MaterialContentResponse {
    normalize_reader_result_with_options(
        target,
        markdown,
        reader_content_type,
        0,
        DEFAULT_MAX_CHARS,
    )
}

pub(crate) fn normalize_reader_result_with_options(
    target: TargetFacts,
    markdown: String,
    reader_content_type: String,
    offset: usize,
    max_chars: usize,
) -> MaterialContentResponse {
    let (status, reason) = if let Some(reason) = challenge_reason(&markdown) {
        (MaterialStatus::Blocked, Some(reason.to_owned()))
    } else if markdown.trim().is_empty() {
        (MaterialStatus::Empty, None)
    } else {
        (MaterialStatus::Extracted, None)
    };
    let part = chunk::slice(&markdown, offset, max_chars);
    let title = title(&markdown);
    let response_links = links::collect(&part.markdown, &target.final_url);

    MaterialContentResponse {
        target,
        extraction: ExtractionResult {
            status,
            engine: "reader_auto".to_owned(),
            format: "markdown".to_owned(),
            reason,
            reader_content_type: Some(reader_content_type),
            title,
            markdown: part.markdown,
            links: response_links,
        },
        download: DownloadCapability { available: true },
        pagination: Pagination {
            offset: part.offset,
            next_offset: part.next_offset,
            truncated: part.next_offset.is_some(),
        },
        diagnostics: Diagnostics {
            upstream_status: None,
            duration_ms: None,
            timeout_seconds: None,
            warnings: Vec::new(),
        },
    }
}

fn challenge_reason(markdown: &str) -> Option<&'static str> {
    let lower = markdown.to_ascii_lowercase();
    if lower.contains("just a moment")
        || lower.contains("cf-chl-")
        || lower.contains("target url returned error 403")
    {
        Some("Reader 返回了挑战页或上游 403")
    } else {
        None
    }
}

fn title(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
}
