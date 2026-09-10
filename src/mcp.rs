use std::sync::Arc;

use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
        ResourceContents, ServerCapabilities, ServerInfo,
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
    pub query: String,
    pub page: Option<u32>,
    pub limit: Option<u8>,
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UrlInput {
    pub url: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadInput {
    pub url: String,
    pub offset: Option<usize>,
    pub max_chars: Option<usize>,
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
    StreamableHttpService::new(
        move || Ok(RefineryMcp::new(state.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(["0.0.0.0", "localhost", "127.0.0.1"])
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
            page: input.page.unwrap_or(1),
            limit: input.limit.unwrap_or(10),
            language: input.language,
        };
        let result = crate::application::search(&self.state, request)
            .await
            .map_err(error)?;
        Ok(CallToolResult::structured(
            serde_json::to_value(result)
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?,
        ))
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
            offset: input.offset.unwrap_or(0),
            max_chars: input.max_chars.unwrap_or(12000),
        };
        let result = crate::application::read(&self.state, request)
            .await
            .map_err(error)?;
        Ok(CallToolResult::structured(
            serde_json::to_value(result)
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?,
        ))
    }

    #[tool(
        name = "web_explore",
        description = "Discover robots, sitemaps, and same-origin page links."
    )]
    async fn web_explore(
        &self,
        Parameters(input): Parameters<UrlInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let request = crate::sitemap::SitemapRequest {
            url: input.url,
            limit: 100,
        };
        let result = crate::application::explore(&self.state, request)
            .await
            .map_err(error)?;
        Ok(CallToolResult::structured(
            serde_json::to_value(result)
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?,
        ))
    }

    #[tool(
        name = "web_download",
        description = "Create a stateless resource reference for a public file."
    )]
    async fn web_download(
        &self,
        Parameters(input): Parameters<UrlInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let url = crate::application::validate_download(&input.url).map_err(error)?;
        Ok(CallToolResult::structured(serde_json::json!({
            "requested_url": url.as_str(),
            "resource_uri": format!("refinery://resource/{}", url::form_urlencoded::byte_serialize(url.as_str().as_bytes()).collect::<String>()),
            "available": true,
            "persistent": false
        })))
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
        let url = crate::content::validate_public_url(&raw).map_err(error)?;
        let resource = self
            .state
            .resource_fetcher
            .get_with_timeout(&url, self.state.config.resource_timeout)
            .await
            .map_err(error)?;
        let blob = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(resource.bytes)
        };
        Ok(ReadResourceResponse::Complete(ReadResourceResult::new(
            vec![ResourceContents::blob(blob, request.uri).with_mime_type(resource.content_type)],
        )))
    }
}

fn error(error: crate::error::ApiError) -> rmcp::ErrorData {
    rmcp::ErrorData::invalid_params(error.message, None)
}
