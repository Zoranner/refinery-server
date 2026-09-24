use crate::{
    content::{
        chunk,
        links::{self, LinkProjection},
    },
    material::{
        Diagnostics, DownloadCapability, ExtractionResult, MaterialContentResponse, MaterialStatus,
        OutputStats, Pagination, TargetFacts,
    },
};

const DEFAULT_MAX_CHARS: usize = 8000;
pub(crate) const MAX_RESPONSE_LINKS: usize = 50;

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
        LinkProjection::default(),
    )
}

pub(crate) fn normalize_reader_result_with_options(
    target: TargetFacts,
    markdown: String,
    reader_content_type: String,
    offset: usize,
    max_chars: usize,
    projection: LinkProjection,
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
    let candidates = links::collect(&part.markdown, &target.final_url)
        .into_iter()
        .filter(|link| match projection {
            LinkProjection::All => true,
            LinkProjection::Resources => link.is_resource(),
        })
        .collect::<Vec<_>>();
    let links_omitted = candidates.len().saturating_sub(MAX_RESPONSE_LINKS);
    let response_links = candidates
        .into_iter()
        .take(MAX_RESPONSE_LINKS)
        .collect::<Vec<_>>();
    let stats = OutputStats {
        markdown_chars: part.markdown.chars().count(),
        links_included: response_links.len(),
        links_omitted,
    };
    let next_offset = part.next_offset;
    let offset = part.offset;

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
            offset,
            next_offset,
            truncated: next_offset.is_some(),
        },
        diagnostics: Diagnostics {
            upstream_status: None,
            duration_ms: None,
            timeout_seconds: None,
            warnings: Vec::new(),
        },
        stats,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ResourceKind;
    use crate::content::links::LinkProjection;
    use url::Url;

    fn target() -> TargetFacts {
        let url = Url::parse("https://example.com/page").unwrap();
        TargetFacts::new(url.clone(), url, ResourceKind::Html, "text/html".to_owned())
    }

    #[test]
    fn resources_projection_keeps_only_resources_and_deduplicates() {
        let markdown = "# 标题\n\n\
            [导航](https://example.com/nav)\n\
            [导航重复](https://example.com/nav)\n\
            [手册](https://example.com/report.pdf)\n\
            [图纸](https://example.com/diagram.png)\n";
        let response = normalize_reader_result_with_options(
            target(),
            markdown.to_owned(),
            "text/plain; charset=utf-8".to_owned(),
            0,
            8000,
            LinkProjection::Resources,
        );
        let urls = response
            .extraction
            .links
            .iter()
            .map(|link| link.url.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            urls,
            [
                "https://example.com/report.pdf",
                "https://example.com/diagram.png"
            ]
        );
        assert_eq!(response.stats.links_included, 2);
        assert_eq!(response.stats.links_omitted, 0);
    }

    #[test]
    fn all_projection_caps_links_and_reports_omitted() {
        let markdown = (0..60)
            .map(|index| format!("[资源 {index}](https://example.com/file-{index}.pdf)\n"))
            .collect::<String>();
        let response = normalize_reader_result_with_options(
            target(),
            markdown,
            "text/plain; charset=utf-8".to_owned(),
            0,
            8000,
            LinkProjection::All,
        );
        assert_eq!(response.extraction.links.len(), MAX_RESPONSE_LINKS);
        assert_eq!(response.stats.links_included, MAX_RESPONSE_LINKS);
        assert_eq!(response.stats.links_omitted, 60 - MAX_RESPONSE_LINKS);
        assert!(response.stats.markdown_chars > 0);
    }
}
