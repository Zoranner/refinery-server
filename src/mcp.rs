use std::sync::Arc;

use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, Implementation, ReadResourceRequestParams,
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
    #[tool(name = "web_search", description = "Search public web pages and资料.")]
    async fn web_search(
        &self,
        Parameters(input): Parameters<SearchInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::search::SearchRequest {
            query: input.query,
            page: input.page,
            limit: input.limit,
            language: input.language,
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
        let blob = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(resource.bytes)
        };
        Ok(ReadResourceResponse::Complete(ReadResourceResult::new(
            vec![ResourceContents::blob(blob, request.uri).with_mime_type(resource.content_type)],
        )))
    }
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
