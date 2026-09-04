#[test]
fn material_model_keeps_target_kind_separate_from_extraction_media_type() {
    let target = refinery::material::TargetFacts::new(
        "https://example.test/report.pdf".parse().unwrap(),
        "https://example.test/report.pdf".parse().unwrap(),
        refinery::content::ResourceKind::Pdf,
        "text/plain; charset=utf-8".to_owned(),
    );
    let extraction =
        refinery::material::ExtractionResult::blocked("reader_auto", "目标站点返回挑战页");

    assert_eq!(target.resource_kind, refinery::content::ResourceKind::Pdf);
    assert_eq!(target.content_type, "text/plain; charset=utf-8");
    assert_eq!(
        extraction.status,
        refinery::material::MaterialStatus::Blocked
    );
}

#[test]
fn material_response_supports_capabilities_pagination_diagnostics_and_json_serialization() {
    let response = refinery::material::MaterialContentResponse {
        target: refinery::material::TargetFacts::new(
            "https://example.test/report.pdf".parse().unwrap(),
            "https://example.test/report.pdf".parse().unwrap(),
            refinery::content::ResourceKind::Pdf,
            "application/pdf".to_owned(),
        ),
        extraction: refinery::material::ExtractionResult::download_only(
            "reader_auto",
            "PDF 不提供可抽取文本",
        ),
        download: refinery::material::DownloadCapability {
            available: true,
            resource_url: Some(
                "/v1/resource?url=https%3A%2F%2Fexample.test%2Freport.pdf".to_owned(),
            ),
        },
        pagination: refinery::material::Pagination {
            offset: 100,
            next_offset: Some(200),
            truncated: true,
        },
        diagnostics: refinery::material::Diagnostics {
            upstream_status: Some(200),
            duration_ms: Some(42),
            timeout_seconds: Some(60),
            warnings: vec!["内容来自 Reader".to_owned()],
        },
    };

    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["target"]["resource_kind"], "pdf");
    assert_eq!(json["target"]["content_type"], "application/pdf");
    assert_eq!(json["extraction"]["status"], "download_only");
    assert_eq!(json["download"]["available"], true);
    assert_eq!(
        json["download"]["resource_url"],
        "/v1/resource?url=https%3A%2F%2Fexample.test%2Freport.pdf"
    );
    assert_eq!(json["pagination"]["next_offset"], 200);
    assert_eq!(json["diagnostics"]["upstream_status"], 200);
    assert_eq!(json["diagnostics"]["warnings"][0], "内容来自 Reader");
}
