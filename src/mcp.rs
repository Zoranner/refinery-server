use std::sync::Arc;

use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, Implementation, MetaObject, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
        ServerInfo,
    },
    schemars::{self, JsonSchema},
    tool, tool_handler, tool_router,
};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Clone)]
pub struct RefineryMcp {
    tool_router: ToolRouter<Self>,
    state: Arc<AppState>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchInput {
    /// 搜索关键词，不能为空。
    pub query: String,
    /// 页码，从 1 开始。
    #[serde(default = "default_page")]
    #[schemars(range(min = 1))]
    pub page: u32,
    /// 返回条数上限。
    #[serde(default = "default_search_limit")]
    #[schemars(range(min = 1, max = 20))]
    pub limit: u8,
    /// SearXNG 语言代码，例如 zh-CN。
    pub language: Option<String>,
    /// SearXNG 类别，逗号分隔，例如 it 或 general,it；默认 general。
    pub categories: Option<String>,
    /// 安全搜索级别：0 关闭、1 中等、2 严格。
    #[serde(default = "default_safesearch")]
    #[schemars(range(min = 0, max = 2))]
    pub safesearch: u8,
    /// 检索意图：web 泛资料（默认）、code 技术资料、qa 排障问答、package 包查询、paper 学术资料。
    pub scope: Option<crate::search::SearchScope>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UrlInput {
    pub url: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadInput {
    /// 目标公网 URL。
    pub url: String,
    /// 已抽取正文中的起始字符位置，续读时使用上次响应的 next_offset。
    #[serde(default = "default_offset")]
    pub offset: usize,
    /// 本次返回的正文最大字符数。
    #[serde(default = "default_max_chars")]
    #[schemars(range(min = 1000, max = 24000))]
    pub max_chars: usize,
    /// 链接投影模式：resources 只返回资源型链接，all 返回全部链接。
    #[serde(default)]
    pub links: crate::content::links::LinkProjection,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExploreInput {
    /// 站点内任意 URL，服务只发现同源地址。
    pub url: String,
    /// 最多发现的 URL 数量。
    #[serde(default = "default_explore_limit")]
    #[schemars(range(min = 1, max = 500))]
    pub limit: usize,
}

fn default_page() -> u32 {
    1
}

fn default_search_limit() -> u8 {
    10
}

fn default_safesearch() -> u8 {
    1
}

fn default_offset() -> usize {
    0
}

fn default_max_chars() -> usize {
    8000
}

fn default_explore_limit() -> usize {
    100
}

impl RefineryMcp {
    pub fn new(state: AppState) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state: Arc::new(state),
        }
    }
}

pub fn http_service(state: AppState) -> StreamableHttpService<RefineryMcp, LocalSessionManager> {
    let allowed_hosts = state.config.mcp_allowed_hosts.clone();
    let allowed_origins = state.config.mcp_allowed_origins.clone();
    StreamableHttpService::new(
        move || Ok(RefineryMcp::new(state.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts)
            .with_allowed_origins(allowed_origins)
            .with_json_response(true),
    )
}

#[tool_router]
impl RefineryMcp {
    #[tool(
        name = "web_search",
        description = "Search public web pages. Set scope=code, qa, package or paper to route the query to specialised engines; the default web scope uses the instance general category and reports when it is degraded."
    )]
    async fn web_search(
        &self,
        Parameters(input): Parameters<SearchInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::search::SearchRequest {
            query: input.query,
            page: input.page,
            limit: input.limit,
            language: input.language,
            categories: input.categories,
            safesearch: Some(input.safesearch),
            scope: input.scope,
        };
        let result = match crate::application::search(&self.state, request).await {
            Ok(result) => result,
            Err(error) => return fail(error),
        };
        let value = serde_json::to_value(result)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        Ok(payload(value))
    }

    #[tool(
        name = "web_read",
        description = "Read public web content as Markdown."
    )]
    async fn web_read(
        &self,
        Parameters(input): Parameters<ReadInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::content::ContentRequest {
            url: input.url,
            offset: input.offset,
            max_chars: input.max_chars,
            links: input.links,
        };
        let result = match crate::application::read(&self.state, request).await {
            Ok(result) => result,
            Err(error) => return fail(error),
        };
        let value = serde_json::to_value(result)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        Ok(payload(value))
    }

    #[tool(
        name = "web_explore",
        description = "Discover robots, sitemaps, and same-origin page links."
    )]
    async fn web_explore(
        &self,
        Parameters(input): Parameters<ExploreInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::sitemap::SitemapRequest {
            url: input.url,
            limit: input.limit,
        };
        let result = match crate::application::explore(&self.state, request).await {
            Ok(result) => result,
            Err(error) => return fail(error),
        };
        let value = serde_json::to_value(result)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        Ok(payload(value))
    }

    #[tool(
        name = "web_download",
        description = "Create a stateless resource reference for a public file."
    )]
    async fn web_download(
        &self,
        Parameters(input): Parameters<UrlInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let url = match crate::application::validate_download(&input.url) {
            Ok(url) => url,
            Err(error) => return fail(error),
        };
        let uri = format!(
            "refinery://resource/{}",
            url::form_urlencoded::byte_serialize(url.as_str().as_bytes()).collect::<String>()
        );
        Ok(CallToolResult::success(vec![ContentBlock::resource_link(
            Resource::new(&uri, "web_download")
                .with_description("Read this resource through MCP resources/read."),
        )]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RefineryMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(
            "Refinery provides controlled public web search, reading, exploration, and downloads.",
        )
        .with_server_info(Implementation::new("refinery", env!("CARGO_PKG_VERSION")))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ReadResourceResponse, rmcp::ErrorData> {
        let encoded = request
            .uri
            .strip_prefix("refinery://resource/")
            .ok_or_else(|| rmcp::ErrorData::invalid_params("unsupported resource URI", None))?;
        let raw: String = url::form_urlencoded::parse(encoded.as_bytes())
            .map(|(key, value)| if key.is_empty() { value } else { key })
            .collect();
        let url = crate::content::validate_public_url(&raw).map_err(resource_error)?;
        let resource = self
            .state
            .resource_fetcher
            .get_with_timeout(&url, self.state.config.resource_timeout)
            .await
            .map_err(resource_error)?;
        let size_bytes = resource.bytes.len();
        let blob = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(resource.bytes)
        };
        Ok(ReadResourceResponse::Complete(ReadResourceResult::new(
            vec![
                ResourceContents::blob(blob, request.uri)
                    .with_mime_type(resource.content_type)
                    .with_meta(resource_meta(resource.final_url.as_str(), size_bytes)),
            ],
        )))
    }
}

fn resource_meta(final_url: &str, size_bytes: usize) -> MetaObject {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "final_url".to_owned(),
        serde_json::Value::String(final_url.to_owned()),
    );
    meta.insert("size_bytes".to_owned(), serde_json::Value::from(size_bytes));
    MetaObject::from(meta)
}

fn payload(value: serde_json::Value) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(value.to_string())])
}

fn fail(error: crate::error::ApiError) -> Result<CallToolResult, rmcp::ErrorData> {
    if error.is_protocol_error() {
        return Err(rmcp::ErrorData::invalid_params(error.message, None));
    }

    Ok(CallToolResult::structured_error(serde_json::json!({
        "error": {
            "code": error.code,
            "message": error.message,
            "stage": error.stage,
            "retryable": error.retryable,
        }
    })))
}

fn resource_error(error: crate::error::ApiError) -> rmcp::ErrorData {
    if error.is_protocol_error() || error.code == "blocked_target" {
        return rmcp::ErrorData::invalid_params(error.message, None);
    }

    rmcp::ErrorData::internal_error(error.message, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_meta_carries_final_url_and_size() {
        let meta = resource_meta("https://example.com/final.pdf", 2048);
        assert_eq!(
            meta.0["final_url"],
            serde_json::json!("https://example.com/final.pdf")
        );
        assert_eq!(meta.0["size_bytes"], serde_json::json!(2048));
    }
}
