use crate::{
    commands::tool_command,
    jobs::Scheduler,
    models::{Health, SubmitJob, ToolRequest},
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
use std::{collections::BTreeMap, net::SocketAddr};
use tokio_util::io::ReaderStream;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

pub async fn serve(address: SocketAddr, scheduler: Scheduler) -> AnyResult<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/monitor", get(index))
        .route("/health", get(health))
        .route("/api/jobs", post(submit).get(list))
        .route("/api/jobs/{id}", get(get_job))
        .route("/api/jobs/{id}/cancel", post(cancel))
        .route("/api/jobs/{id}/pause", post(pause))
        .route("/api/jobs/{id}/resume", post(resume))
        .route("/api/jobs/{id}/kill", post(kill))
        .route("/api/jobs/{id}/output", get(output))
        .route("/api/jobs/{id}/tail", get(tail))
        .route("/api/jobs/{id}/logs/{stream}", get(download_log))
        .route("/api/command", post(legacy_command))
        .route("/api/tools/{tool}", post(submit_tool))
        .layer(DefaultBodyLimit::max(512 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(scheduler);
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

async fn submit(
    State(scheduler): State<Scheduler>,
    Json(request): Json<SubmitJob>,
) -> Result<impl IntoResponse, ApiError> {
    let job = scheduler.submit(request).await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn submit_tool(
    State(scheduler): State<Scheduler>,
    Path(tool): Path<String>,
    Json(mut request): Json<ToolRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let timeout_seconds = take_u64(&mut request.values, "timeout_seconds")?;
    let webhook_url = take_string(&mut request.values, "webhook_url")?;
    let argv = tool_command(&tool, &request)?;
    let job = scheduler
        .submit(SubmitJob {
            tool: Some(tool),
            argv,
            timeout_seconds,
            webhook_url,
        })
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn legacy_command(
    State(scheduler): State<Scheduler>,
    Json(mut request): Json<ToolRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let timeout_seconds = take_u64(&mut request.values, "timeout_seconds")?;
    let webhook_url = take_string(&mut request.values, "webhook_url")?;
    let command = take_string(&mut request.values, "command")?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "command is required".into()))?;
    let argv = shell_words::split(&command)
        .map_err(|error| ApiError(StatusCode::BAD_REQUEST, error.to_string()))?;
    let job = scheduler
        .submit(SubmitJob {
            tool: Some("command".into()),
            argv,
            timeout_seconds,
            webhook_url,
        })
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

fn take_u64(values: &mut BTreeMap<String, Value>, key: &str) -> Result<Option<u64>, ApiError> {
    match values.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_u64().map(Some).ok_or_else(|| {
            ApiError(
                StatusCode::BAD_REQUEST,
                format!("{key} must be a positive integer"),
            )
        }),
        Some(_) => Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!("{key} must be an integer"),
        )),
    }
}

fn take_string(
    values: &mut BTreeMap<String, Value>,
    key: &str,
) -> Result<Option<String>, ApiError> {
    match values.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!("{key} must be a string"),
        )),
    }
}

async fn list(State(scheduler): State<Scheduler>) -> Json<Value> {
    Json(json!({"jobs": scheduler.list().await}))
}

async fn get_job(
    State(scheduler): State<Scheduler>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    scheduler
        .get(id)
        .await
        .map(|j| Json(json!(j)))
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "job not found".into()))
}

async fn cancel(
    State(scheduler): State<Scheduler>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    scheduler
        .cancel(id)
        .await
        .map(|j| Json(json!(j)))
        .map_err(|e| ApiError(StatusCode::CONFLICT, e.to_string()))
}

async fn pause(
    State(scheduler): State<Scheduler>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    scheduler
        .pause(id)
        .await
        .map(|j| Json(json!(j)))
        .map_err(|error| ApiError(StatusCode::CONFLICT, error.to_string()))
}

async fn resume(
    State(scheduler): State<Scheduler>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    scheduler
        .resume(id)
        .await
        .map(|j| Json(json!(j)))
        .map_err(|error| ApiError(StatusCode::CONFLICT, error.to_string()))
}

async fn kill(
    State(scheduler): State<Scheduler>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    scheduler
        .kill(id)
        .await
        .map(|j| Json(json!(j)))
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
    State(scheduler): State<Scheduler>,
    Path(id): Path<Uuid>,
    Query(query): Query<OutputQuery>,
) -> Result<Json<Value>, ApiError> {
    scheduler
        .output(id, &query.stream, query.offset, query.limit)
        .await
        .map(|o| Json(json!(o)))
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
    State(scheduler): State<Scheduler>,
    Path(id): Path<Uuid>,
    Query(query): Query<TailQuery>,
) -> Result<Json<Value>, ApiError> {
    scheduler
        .tail(id, &query.stream, query.lines)
        .await
        .map(|data| Json(json!({"job_id": id, "stream": query.stream, "lines": query.lines.clamp(1, 500), "data": data})))
        .map_err(ApiError::from)
}

async fn download_log(
    State(scheduler): State<Scheduler>,
    Path((id, stream)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let filename = format!("mcp-kali-{id}-{stream}.log");
    let body = match scheduler.open_log(id, &stream).await? {
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

async fn health(State(scheduler): State<Scheduler>) -> Json<Health> {
    let (queued, running, max_concurrency) = scheduler.counts().await;
    Json(Health {
        status: "healthy",
        service: "mcp-kali",
        version: env!("CARGO_PKG_VERSION"),
        queued,
        running,
        max_concurrency,
    })
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
