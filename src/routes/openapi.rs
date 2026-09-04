use axum::Json;
use serde_json::{Value, json};

pub async fn document() -> Json<Value> {
    Json(json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Refinery API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "内网调用方使用的外网资料搜索、抽取、资源下载和站点地图接口"
        },
        "paths": {
            "/health": {
                "get": {
                    "responses": {
                        "200": {
                            "description": "服务正常",
                            "content": { "application/json": { "schema": { "$ref": "#/components/schemas/HealthResponse" } } }
                        }
                    }
                }
            },
            "/openapi.json": {
                "get": {
                    "responses": {
                        "200": {
                            "description": "OpenAPI 契约",
                            "content": { "application/json": { "schema": { "type": "object" } } }
                        }
                    }
                }
            },
            "/v1/search": {
                "post": {
                    "requestBody": { "$ref": "#/components/requestBodies/SearchRequest" },
                    "responses": {
                        "200": { "description": "搜索结果", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SearchResponse" } } } },
                        "400": { "$ref": "#/components/responses/InvalidRequest" },
                        "502": { "$ref": "#/components/responses/SearchUpstreamFailed" }
                    }
                }
            },
            "/v1/content": {
                "post": {
                    "requestBody": { "$ref": "#/components/requestBodies/ContentRequest" },
                    "responses": {
                        "200": { "description": "Markdown 内容", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ContentResponse" } } } },
                        "400": { "$ref": "#/components/responses/InvalidRequest" },
                        "403": { "$ref": "#/components/responses/BlockedTarget" },
                        "415": { "$ref": "#/components/responses/ResourceDownloadRequired" },
                        "502": { "$ref": "#/components/responses/FetchFailed" },
                        "504": { "$ref": "#/components/responses/FetchTimeout" }
                    }
                }
            },
            "/v1/resource": {
                "get": {
                    "parameters": [{ "name": "url", "in": "query", "required": true, "schema": { "type": "string", "format": "uri" } }],
                    "responses": {
                        "200": { "description": "原始资源", "headers": { "Content-Disposition": { "schema": { "type": "string" } }, "X-Content-Type-Options": { "schema": { "type": "string", "example": "nosniff" } } }, "content": { "application/octet-stream": { "schema": { "type": "string", "format": "binary" } } } },
                        "400": { "$ref": "#/components/responses/InvalidRequest" },
                        "403": { "$ref": "#/components/responses/BlockedTarget" },
                        "413": { "$ref": "#/components/responses/ResponseTooLarge" },
                        "502": { "$ref": "#/components/responses/FetchFailed" },
                        "504": { "$ref": "#/components/responses/FetchTimeout" }
                    }
                }
            },
            "/v1/sitemap": {
                "post": {
                    "requestBody": { "$ref": "#/components/requestBodies/SitemapRequest" },
                    "responses": {
                        "200": { "description": "站点地图结果", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SitemapResponse" } } } },
                        "400": { "$ref": "#/components/responses/InvalidRequest" },
                        "403": { "$ref": "#/components/responses/BlockedTarget" },
                        "502": { "$ref": "#/components/responses/FetchFailed" },
                        "504": { "$ref": "#/components/responses/FetchTimeout" }
                    }
                }
            }
        },
        "components": {
            "requestBodies": {
                "SearchRequest": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SearchRequest" } } } },
                "ContentRequest": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ContentRequest" } } } },
                "SitemapRequest": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SitemapRequest" } } } }
            },
            "responses": {
                "InvalidRequest": { "description": "请求参数无效", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorResponse" } } } },
                "BlockedTarget": { "description": "目标地址不允许访问", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorResponse" } } } },
                "ResourceDownloadRequired": { "description": "应改用资源下载接口", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ResourceDownloadError" } } } },
                "SearchUpstreamFailed": { "description": "搜索上游不可用", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorResponse" } } } },
                "FetchFailed": { "description": "内容上游不可用", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorResponse" } } } },
                "FetchTimeout": { "description": "内容上游超时", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorResponse" } } } },
                "ResponseTooLarge": { "description": "上游响应超过大小限制", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ErrorResponse" } } } }
            },
            "schemas": {
                "HealthResponse": { "type": "object", "required": ["status"], "properties": { "status": { "type": "string", "example": "ok" } } },
                "SearchRequest": { "type": "object", "required": ["query"], "properties": { "query": { "type": "string", "minLength": 1 }, "page": { "type": "integer", "minimum": 1, "default": 1 }, "limit": { "type": "integer", "minimum": 1, "maximum": 20, "default": 10 }, "language": { "type": "string", "nullable": true, "example": "zh-CN" } } },
                "SearchResponse": { "type": "object", "required": ["query", "page", "results"], "properties": { "query": { "type": "string" }, "page": { "type": "integer" }, "results": { "type": "array", "items": { "$ref": "#/components/schemas/SearchResult" } } } },
                "SearchResult": { "type": "object", "required": ["title", "url", "snippet"], "properties": { "title": { "type": "string" }, "url": { "type": "string", "format": "uri" }, "snippet": { "type": "string" }, "published_at": { "type": "string", "nullable": true } } },
                "ContentRequest": { "type": "object", "required": ["url"], "properties": { "url": { "type": "string", "format": "uri" }, "offset": { "type": "integer", "minimum": 0, "default": 0 }, "max_chars": { "type": "integer", "minimum": 1000, "maximum": 24000, "default": 12000 } } },
                "ContentResponse": { "type": "object", "required": ["requested_url", "final_url", "content_kind", "content_type", "markdown", "links", "offset", "truncated", "warnings"], "properties": { "requested_url": { "type": "string", "format": "uri" }, "final_url": { "type": "string", "format": "uri" }, "content_kind": { "type": "string", "enum": ["html", "text", "pdf", "image", "unknown"] }, "content_type": { "type": "string" }, "title": { "type": "string", "nullable": true }, "markdown": { "type": "string" }, "links": { "type": "array", "items": { "$ref": "#/components/schemas/Link" } }, "offset": { "type": "integer" }, "next_offset": { "type": "integer", "nullable": true }, "truncated": { "type": "boolean" }, "warnings": { "type": "array", "items": { "type": "string" } } } },
                "Link": { "type": "object", "required": ["text", "url", "kind"], "properties": { "text": { "type": "string" }, "url": { "type": "string", "format": "uri" }, "kind": { "type": "string", "enum": ["html", "pdf", "image", "unknown"] } } },
                "SitemapRequest": { "type": "object", "required": ["url"], "properties": { "url": { "type": "string", "format": "uri" }, "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 100 } } },
                "SitemapResponse": { "type": "object", "required": ["requested_url", "site_url", "urls", "truncated", "warnings"], "properties": { "requested_url": { "type": "string", "format": "uri" }, "site_url": { "type": "string", "format": "uri" }, "urls": { "type": "array", "items": { "$ref": "#/components/schemas/SitemapUrl" } }, "truncated": { "type": "boolean" }, "warnings": { "type": "array", "items": { "type": "string" } } } },
                "SitemapUrl": { "type": "object", "required": ["url", "source"], "properties": { "url": { "type": "string", "format": "uri" }, "source": { "type": "string", "enum": ["sitemap", "page_link"] } } },
                "Error": { "type": "object", "required": ["code", "message"], "properties": { "code": { "type": "string" }, "message": { "type": "string" } } },
                "ErrorResponse": { "type": "object", "required": ["error"], "properties": { "error": { "$ref": "#/components/schemas/Error" } } },
                "ResourceDownloadError": { "type": "object", "required": ["error", "resource_url"], "properties": { "error": { "$ref": "#/components/schemas/Error" }, "resource_url": { "type": "string", "format": "uri-reference" } } }
            }
        }
    }))
}
