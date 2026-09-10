use std::sync::Arc;

use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
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
        request.validate().map_err(error)?;
        let result = crate::search::searxng::search(
            &self.state.http_client,
            &self.state.config.searxng_base_url,
            &request,
        )
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
        let url = request.validate().map_err(error)?;
        let result = crate::reader::client::read(
            &self.state.http_client,
            &self.state.config.reader_base_url,
            &url,
            self.state.config.reader_timeout,
        )
        .await
        .map_err(error)?;
        let kind = crate::content::kind_for_response(&result.final_url, &result.content_type);
        let target = crate::material::TargetFacts::new(
            request
                .url
                .parse()
                .map_err(|_| error(crate::error::ApiError::invalid_request("url is invalid")))?,
            result.final_url,
            kind,
            result.content_type.clone(),
        );
        let result =
            crate::material::normalize_reader_result(target, result.markdown, result.content_type);
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
        let result = crate::sitemap::discover(self.state.sitemap_fetcher.clone(), request)
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
        let url = crate::content::validate_public_url(&input.url).map_err(error)?;
        Ok(CallToolResult::structured(serde_json::json!({
            "requested_url": url.as_str(),
            "resource_uri": format!("refinery://resource/{}", url::form_urlencoded::byte_serialize(url.as_str().as_bytes()).collect::<String>()),
            "available": true,
            "persistent": false
        })))
    }
}

#[tool_handler]
impl ServerHandler for RefineryMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Refinery provides controlled public web search, reading, exploration, and downloads.",
        )
    }
}

fn error(error: crate::error::ApiError) -> rmcp::ErrorData {
    rmcp::ErrorData::invalid_params(error.message, None)
}
