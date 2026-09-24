use std::collections::HashSet;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkProjection {
    /// 只返回 PDF、图片和未知附件，页面链接由 Markdown 内联表达。
    #[default]
    Resources,
    /// 返回当前分段中的全部链接。
    All,
}

#[derive(Debug, Serialize)]
pub struct Link {
    pub text: String,
    pub url: String,
    pub kind: LinkKind,
}

impl Link {
    pub fn is_resource(&self) -> bool {
        !matches!(self.kind, LinkKind::Html)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Html,
    Pdf,
    Image,
    Unknown,
}

pub fn collect(markdown: &str, base_url: &Url) -> Vec<Link> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    let mut current: Option<(String, String)> = None;

    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                current = Some((String::new(), dest_url.into_string()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((label, _)) = &mut current {
                    label.push_str(&text);
                }
            }
            Event::End(TagEnd::Link)
                if let Some((text, target)) = current.take()
                    && let Some(url) = resolve(base_url, &target)
                    && seen.insert(url.to_string()) =>
            {
                links.push(Link {
                    text,
                    kind: classify(&url),
                    url: url.to_string(),
                });
            }
            _ => {}
        }
    }

    links
}

fn resolve(base_url: &Url, target: &str) -> Option<Url> {
    Url::parse(target).or_else(|_| base_url.join(target)).ok()
}

fn classify(url: &Url) -> LinkKind {
    let path = url.path().to_ascii_lowercase();

    if path.ends_with(".pdf") {
        LinkKind::Pdf
    } else if [".avif", ".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp"]
        .iter()
        .any(|extension| path.ends_with(extension))
    {
        LinkKind::Image
    } else if matches!(url.scheme(), "http" | "https") {
        LinkKind::Html
    } else {
        LinkKind::Unknown
    }
}
