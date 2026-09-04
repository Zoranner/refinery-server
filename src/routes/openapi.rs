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
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/HealthResponse" }
                                }
                            }
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
                        "200": {
                            "description": "搜索结果",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/SearchResponse" }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/InvalidRequest" },
                        "502": { "$ref": "#/components/responses/SearchUpstreamFailed" }
                    }
                }
            },
            "/v1/content": {
                "post": {
                    "requestBody": { "$ref": "#/components/requestBodies/ContentRequest" },
                    "responses": {
                        "200": {
                            "description": "资料事实、抽取结果、下载能力和分段信息",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/MaterialContentResponse" }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/InvalidRequest" },
                        "403": { "$ref": "#/components/responses/BlockedTarget" },
                        "413": { "$ref": "#/components/responses/ResponseTooLarge" },
                        "502": { "$ref": "#/components/responses/FetchFailed" },
                        "504": { "$ref": "#/components/responses/FetchTimeout" }
                    }
                }
            },
            "/v1/resource": {
                "get": {
                    "parameters": [{
                        "name": "url",
                        "in": "query",
                        "required": true,
                        "schema": { "type": "string", "format": "uri" }
                    }],
                    "responses": {
                        "200": {
                            "description": "原始资源",
                            "headers": {
                                "Content-Disposition": { "schema": { "type": "string" } },
                                "X-Content-Type-Options": {
                                    "schema": { "type": "string", "example": "nosniff" }
                                }
                            },
                            "content": {
                                "application/octet-stream": {
                                    "schema": { "type": "string", "format": "binary" }
                                }
                            }
                        },
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
                        "200": {
                            "description": "站点地图结果",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/SitemapResponse" }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/InvalidRequest" },
                        "403": { "$ref": "#/components/responses/BlockedTarget" },
                        "502": { "$ref": "#/components/responses/FetchFailed" }
                    }
                }
            }
        },
        "components": {
            "requestBodies": {
                "SearchRequest": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/SearchRequest" }
                        }
                    }
                },
                "ContentRequest": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ContentRequest" }
                        }
                    }
                },
                "SitemapRequest": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/SitemapRequest" }
                        }
                    }
                }
            },
            "responses": {
                "InvalidRequest": {
                    "description": "请求参数无效，统一返回 JSON 错误体",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                        }
                    }
                },
                "BlockedTarget": {
                    "description": "目标地址不允许访问",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                        }
                    }
                },
                "SearchUpstreamFailed": {
                    "description": "搜索上游不可用",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                        }
                    }
                },
                "FetchFailed": {
                    "description": "内容上游不可用",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                        }
                    }
                },
                "FetchTimeout": {
                    "description": "内容上游超时",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                        }
                    }
                },
                "ResponseTooLarge": {
                    "description": "上游响应超过大小限制",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                        }
                    }
                },
                "UnsupportedMediaType": {
                    "description": "仍保留该状态码供非内容业务使用",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                        }
                    }
                }
            },
            "schemas": {
                "HealthResponse": {
                    "type": "object",
                    "required": ["status"],
                    "properties": { "status": { "type": "string", "example": "ok" } }
                },
                "SearchRequest": {
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": { "type": "string", "minLength": 1 },
                        "page": { "type": "integer", "minimum": 1, "default": 1 },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 20, "default": 10 },
                        "language": { "type": "string", "nullable": true, "example": "zh-CN" }
                    }
                },
                "SearchResponse": {
                    "type": "object",
                    "required": ["query", "page", "results", "pagination", "diagnostics"],
                    "properties": {
                        "query": { "type": "string" },
                        "page": { "type": "integer" },
                        "results": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/SearchResult" }
                        },
                        "pagination": { "$ref": "#/components/schemas/SearchPagination" },
                        "diagnostics": { "$ref": "#/components/schemas/SearchDiagnostics" }
                    }
                },
                "SearchPagination": {
                    "type": "object",
                    "description": "has_more=null 表示上游没有提供可信的分页结束信号；空页不代表已完成。",
                    "required": ["requested_page", "has_more"],
                    "properties": {
                        "requested_page": { "type": "integer", "minimum": 1 },
                        "has_more": { "type": "boolean", "nullable": true }
                    }
                },
                "SearchDiagnostics": {
                    "type": "object",
                    "required": ["source_status", "warnings"],
                    "properties": {
                        "source_status": { "type": "string", "example": "ok" },
                        "warnings": { "type": "array", "items": { "type": "string" } }
                    }
                },
                "SearchResult": {
                    "type": "object",
                    "required": ["title", "url", "snippet"],
                    "properties": {
                        "title": { "type": "string" },
                        "url": { "type": "string", "format": "uri" },
                        "snippet": { "type": "string" },
                        "published_at": { "type": "string", "nullable": true }
                    }
                },
                "ContentRequest": {
                    "type": "object",
                    "required": ["url"],
                    "properties": {
                        "url": { "type": "string", "format": "uri" },
                        "offset": { "type": "integer", "minimum": 0, "default": 0 },
                        "max_chars": {
                            "type": "integer",
                            "minimum": 1000,
                            "maximum": 24000,
                            "default": 12000
                        }
                    }
                },
                "TargetFacts": {
                    "type": "object",
                    "required": ["requested_url", "final_url", "resource_kind", "content_type"],
                    "properties": {
                        "requested_url": { "type": "string", "format": "uri" },
                        "final_url": { "type": "string", "format": "uri" },
                        "resource_kind": {
                            "type": "string",
                            "enum": ["html", "text", "pdf", "image", "unknown"]
                        },
                        "content_type": { "type": "string" }
                    }
                },
                "ExtractionResult": {
                    "type": "object",
                    "required": ["status", "engine", "format", "title", "markdown", "links"],
                    "properties": {
                        "status": {
                            "$ref": "#/components/schemas/MaterialStatus"
                        },
                        "engine": {
                            "type": "string",
                            "description": "Reader 抽取使用 reader_auto；download_only 使用 none。",
                            "example": "reader_auto"
                        },
                        "format": { "type": "string", "example": "markdown" },
                        "reason": { "type": "string", "nullable": true },
                        "reader_content_type": { "type": "string", "nullable": true },
                        "title": { "type": "string", "nullable": true },
                        "markdown": { "type": "string" },
                        "links": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/Link" }
                        }
                    }
                },
                "MaterialStatus": {
                    "type": "string",
                    "enum": ["extracted", "empty", "blocked", "failed", "timed_out", "download_only"]
                },
                "DownloadCapability": {
                    "type": "object",
                    "required": ["available", "resource_url"],
                    "properties": {
                        "available": { "type": "boolean" },
                        "resource_url": { "type": "string", "format": "uri-reference", "nullable": true }
                    }
                },
                "Pagination": {
                    "type": "object",
                    "required": ["offset", "next_offset", "truncated"],
                    "properties": {
                        "offset": { "type": "integer", "minimum": 0 },
                        "next_offset": { "type": "integer", "minimum": 0, "nullable": true },
                        "truncated": { "type": "boolean" }
                    }
                },
                "Diagnostics": {
                    "type": "object",
                    "required": ["upstream_status", "duration_ms", "timeout_seconds", "warnings"],
                    "properties": {
                        "upstream_status": { "type": "integer", "minimum": 100, "maximum": 599, "nullable": true },
                        "duration_ms": { "type": "integer", "minimum": 0, "nullable": true },
                        "timeout_seconds": { "type": "integer", "minimum": 0, "nullable": true },
                        "warnings": { "type": "array", "items": { "type": "string" } }
                    }
                },
                "MaterialContentResponse": {
                    "type": "object",
                    "required": ["target", "extraction", "download", "pagination", "diagnostics"],
                    "properties": {
                        "target": { "$ref": "#/components/schemas/TargetFacts" },
                        "extraction": { "$ref": "#/components/schemas/ExtractionResult" },
                        "download": { "$ref": "#/components/schemas/DownloadCapability" },
                        "pagination": { "$ref": "#/components/schemas/Pagination" },
                        "diagnostics": { "$ref": "#/components/schemas/Diagnostics" }
                    }
                },
                "Link": {
                    "type": "object",
                    "required": ["text", "url", "kind"],
                    "properties": {
                        "text": { "type": "string" },
                        "url": { "type": "string", "format": "uri" },
                        "kind": { "type": "string", "enum": ["html", "pdf", "image", "unknown"] }
                    }
                },
                "SitemapRequest": {
                    "type": "object",
                    "required": ["url"],
                    "properties": {
                        "url": { "type": "string", "format": "uri" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 100 }
                    }
                },
                "SitemapResponse": {
                    "type": "object",
                    "required": ["requested_url", "site_url", "urls", "truncated", "warnings"],
                    "properties": {
                        "requested_url": { "type": "string", "format": "uri" },
                        "site_url": { "type": "string", "format": "uri" },
                        "urls": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/SitemapUrl" }
                        },
                        "truncated": { "type": "boolean" },
                        "warnings": { "type": "array", "items": { "type": "string" } }
                    }
                },
                "SitemapUrl": {
                    "type": "object",
                    "required": ["url", "source"],
                    "properties": {
                        "url": { "type": "string", "format": "uri" },
                        "source": { "type": "string", "enum": ["sitemap", "page_link"] }
                    }
                },
                "Error": {
                    "type": "object",
                    "required": ["code", "message"],
                    "properties": {
                        "code": { "type": "string" },
                        "message": { "type": "string" }
                    }
                },
                "ErrorResponse": {
                    "type": "object",
                    "required": ["error"],
                    "properties": { "error": { "$ref": "#/components/schemas/Error" } }
                }
            }
        }
    }))
}
