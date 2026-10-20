//! Per-conversation MCP endpoint for the open document, never the saved-file catalog.

use crate::host::ToolHost;
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
};
use rmcp::{
    ServerHandler,
    model::*,
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct LiveTools {
    host: ToolHost,
    active: Arc<AtomicBool>,
    failure: CancellationToken,
}

impl ServerHandler for LiveTools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = ToolHost::definitions()
            .into_iter()
            .map(|definition| {
                Tool::new(
                    definition.name,
                    definition.description,
                    definition
                        .parameters
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let result = if self.active.load(Ordering::Acquire) {
            tokio::select! {
                biased;
                _ = self.failure.cancelled() => Err("The live tool connection closed".into()),
                _ = context.ct.cancelled() => {
                    // End the worker as well: a late approval must not be read as a new prompt.
                    self.failure.cancel();
                    Err("The live tool request was cancelled".into())
                }
                reply = self.host.call(&request.name, serde_json::Value::Object(request.arguments.unwrap_or_default())) => reply,
            }
        } else {
            Err("No active agent turn".into())
        };
        let result = match result {
            Ok(reply) if reply["ok"] == true => {
                if let Some(report) = reply.get("inspection") {
                    match serde_json::from_value(report.clone())
                        .map_err(|e| e.to_string())
                        .and_then(|report| crate::toolbox::audio_inspection::present(&report))
                    {
                        Ok(presentation) => CallToolResult::success(vec![
                            ContentBlock::text(presentation.text),
                            ContentBlock::image(presentation.png, "image/png"),
                        ]),
                        Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
                    }
                } else {
                    CallToolResult::success(vec![ContentBlock::text(
                        reply["text"].as_str().unwrap_or_default(),
                    )])
                }
            }
            Ok(reply) => CallToolResult::error(vec![ContentBlock::text(
                reply["text"].as_str().unwrap_or("The live edit failed"),
            )]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
        };
        Ok(result.into())
    }
}

/// Owns a loopback listener and revokes it when its conversation ends.
pub(crate) struct LiveMcp {
    pub(crate) url: String,
    pub(crate) token: String,
    active: Arc<AtomicBool>,
    failure: CancellationToken,
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl LiveMcp {
    pub(crate) async fn start(host: ToolHost) -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?;
        let url = format!(
            "http://{}/mcp",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let token = uuid::Uuid::new_v4().simple().to_string();
        let authorization = format!("Bearer {token}");
        let active = Arc::new(AtomicBool::new(false));
        let failure = CancellationToken::new();
        let handler = LiveTools {
            host,
            active: active.clone(),
            failure: failure.clone(),
        };
        let cancellation = CancellationToken::new();
        let service: StreamableHttpService<LiveTools, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok(handler.clone()),
                Default::default(),
                StreamableHttpServerConfig::default()
                    .with_json_response(true)
                    .with_cancellation_token(cancellation.clone()),
            );
        let router = Router::new()
            .nest_service("/mcp", service)
            .layer(middleware::from_fn(move |request: Request, next: Next| {
                let authorization = authorization.clone();
                async move {
                    if request.headers().contains_key("origin")
                        || request
                            .headers()
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            != Some(authorization.as_str())
                    {
                        return Response::builder()
                            .status(StatusCode::UNAUTHORIZED)
                            .body(Body::empty())
                            .unwrap_or_default();
                    }
                    next.run(request).await
                }
            }));
        let shutdown = cancellation.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await;
        });
        Ok(Self {
            url,
            token,
            active,
            failure,
            cancellation,
            task,
        })
    }

    pub(crate) fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    pub(crate) async fn cancelled(&self) {
        self.failure.cancelled().await;
    }

    pub(crate) fn abort_calls(&self) {
        self.set_active(false);
        self.failure.cancel();
    }
}

impl Drop for LiveMcp {
    fn drop(&mut self) {
        self.abort_calls();
        self.cancellation.cancel();
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::time::Duration;

    #[tokio::test]
    async fn cancelled_mcp_requests_cannot_execute_after_a_late_approval() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (bridge, commands, events) = crate::worker::test_bridge();
            let mcp = LiveMcp::start(ToolHost::new(bridge)).await.unwrap();
            mcp.set_active(true);
            let client = reqwest::Client::new();
            let request = |value: Value| client.post(&mcp.url).bearer_auth(&mcp.token)
                .header("Accept", "application/json, text/event-stream").json(&value);
            let init = request(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"cancellation-fixture","version":"1"}}})).send().await.unwrap();
            let session = init.headers().get("mcp-session-id").unwrap().to_str().unwrap().to_owned();
            request(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).header("mcp-session-id", &session).send().await.unwrap();
            let call = request(json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"add_track","arguments":{"name":"Late","kind":"instrument"}}})).header("mcp-session-id", &session);
            let running = tokio::spawn(async move { call.send().await.unwrap().text().await.unwrap() });
            let permission = loop {
                if let Ok(event) = events.try_recv() { break event; }
                tokio::task::yield_now().await;
            };
            assert_eq!(permission["event"], "permission");
            request(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":9,"reason":"fixture timeout"}})).header("mcp-session-id", &session).send().await.unwrap();
            mcp.cancelled().await;
            // MCP cancellation may close the SSE response without a result message.
            // Waiting for it also waits for the handler to drop the pending host call.
            let _ = running.await.unwrap();
            commands.send(json!({"event":"permission_result","id":permission["id"],"ok":true}).to_string()).unwrap();
            assert!(events.try_recv().is_err(), "A cancelled call requested a late edit");
        }).await.unwrap();
    }
}
