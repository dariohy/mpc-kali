use crate::{
    jobs::Scheduler,
    models::Health,
    plugins::{InvokeRequest, InvokeResponse, PluginRegistry},
};
use anyhow::Result as AnyResult;
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeSet, net::SocketAddr, sync::Arc};
use tokio_util::io::ReaderStream;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    scheduler: Scheduler,
    registry: Arc<PluginRegistry>,
}

pub async fn serve(
    address: SocketAddr,
    scheduler: Scheduler,
    registry: PluginRegistry,
) -> AnyResult<()> {
    let state = AppState {
        scheduler,
        registry: Arc::new(registry),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/monitor", get(index))
        .route("/health", get(health))
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/{id}", get(get_job))
        .route("/api/jobs/{id}/cancel", post(cancel))
        .route("/api/jobs/{id}/pause", post(pause))
        .route("/api/jobs/{id}/resume", post(resume))
        .route("/api/jobs/{id}/kill", post(kill))
        .route("/api/jobs/{id}/output", get(output))
        .route("/api/jobs/{id}/tail", get(tail))
        .route("/api/jobs/{id}/logs/{stream}", get(download_log))
        .route("/api/plugins", get(plugins))
        .route("/api/plugins/diagnostics", get(plugin_diagnostics))
        .route("/api/plugins/{plugin_id}", get(plugin_get))
        .route("/api/capabilities", get(capabilities))
        .route(
            "/api/capabilities/{capability_id}/tools",
            get(capability_tools),
        )
        .route("/api/tools", get(tools))
        .route("/api/tools/{tool_name}/invoke", post(invoke_tool))
        .layer(DefaultBodyLimit::max(512 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "HTTP server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(StatusCode::BAD_REQUEST, error.to_string())
    }
}

async fn list_jobs(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"jobs": state.scheduler.list().await}))
}

async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    state
        .scheduler
        .get(id)
        .await
        .map(|job| Json(json!(job)))
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "job not found".into()))
}

async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    action_response(state.scheduler.cancel(id).await)
}

async fn pause(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    action_response(state.scheduler.pause(id).await)
}

async fn resume(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    action_response(state.scheduler.resume(id).await)
}

async fn kill(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    action_response(state.scheduler.kill(id).await)
}

fn action_response(result: anyhow::Result<crate::models::Job>) -> Result<Json<Value>, ApiError> {
    result
        .map(|job| Json(json!(job)))
        .map_err(|error| ApiError(StatusCode::CONFLICT, error.to_string()))
}

#[derive(Deserialize)]
struct OutputQuery {
    #[serde(default = "stdout")]
    stream: String,
    #[serde(default)]
    offset: u64,
    #[serde(default = "output_limit")]
    limit: usize,
}

fn stdout() -> String {
    "stdout".into()
}

fn output_limit() -> usize {
    64 * 1024
}

async fn output(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<OutputQuery>,
) -> Result<Json<Value>, ApiError> {
    state
        .scheduler
        .output(id, &query.stream, query.offset, query.limit)
        .await
        .map(|page| Json(json!(page)))
        .map_err(ApiError::from)
}

#[derive(Deserialize)]
struct TailQuery {
    #[serde(default = "stdout")]
    stream: String,
    #[serde(default = "tail_lines")]
    lines: usize,
}

fn tail_lines() -> usize {
    50
}

async fn tail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<TailQuery>,
) -> Result<Json<Value>, ApiError> {
    state
        .scheduler
        .tail(id, &query.stream, query.lines)
        .await
        .map(|data| {
            Json(json!({
                "job_id": id,
                "stream": query.stream,
                "lines": query.lines.clamp(1, 500),
                "data": data
            }))
        })
        .map_err(ApiError::from)
}

async fn download_log(
    State(state): State<AppState>,
    Path((id, stream)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let filename = format!("mcp-kali-{id}-{stream}.log");
    let body = match state.scheduler.open_log(id, &stream).await? {
        Some(file) => Body::from_stream(ReaderStream::new(file)),
        None => Body::empty(),
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(body)
        .map_err(|error| ApiError(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let (queued, running, max_concurrency) = state.scheduler.counts().await;
    Json(Health {
        status: "healthy",
        service: "mcp-kali",
        version: env!("CARGO_PKG_VERSION"),
        queued,
        running,
        max_concurrency,
    })
}

async fn plugins(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"plugins": state.registry.plugins()}))
}

async fn plugin_get(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .registry
        .plugin(&plugin_id)
        .map(|plugin| Json(json!(plugin)))
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "plugin not found".into()))
}

async fn plugin_diagnostics(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"diagnostics": state.registry.diagnostics()}))
}

async fn capabilities(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"capabilities": state.registry.capabilities()}))
}

async fn capability_tools(
    State(state): State<AppState>,
    Path(capability_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let capability = state
        .registry
        .capability(&capability_id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "capability not found".into()))?;
    let projections = state.registry.tools();
    let names = capability
        .providers
        .iter()
        .flat_map(|provider| provider.available_tools.iter())
        .collect::<BTreeSet<_>>();
    let available = names
        .into_iter()
        .filter_map(|name| projections.iter().find(|tool| &tool.name == name).cloned())
        .collect::<Vec<_>>();
    Ok(Json(json!({"capability": capability, "tools": available})))
}

async fn tools(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"tools": state.registry.tools()}))
}

async fn invoke_tool(
    State(state): State<AppState>,
    Path(tool_name): Path<String>,
    Json(request): Json<InvokeRequest>,
) -> Result<Response, ApiError> {
    if !state.registry.has_tool(&tool_name) {
        return Err(ApiError(StatusCode::NOT_FOUND, "tool not found".into()));
    }
    match state
        .registry
        .invoke(&tool_name, request, &state.scheduler)
        .await
        .map_err(ApiError::from)?
    {
        InvokeResponse::Accepted(value) => Ok((StatusCode::ACCEPTED, Json(value)).into_response()),
        InvokeResponse::Immediate(value) => Ok(Json(value).into_response()),
    }
}

async fn index() -> Response {
    let mut response = Html(include_str!("dashboard.html")).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; connect-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
