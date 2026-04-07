use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};
use webclaw_core::ExtractionOptions;
use webclaw_fetch::{FetchClient, FetchConfig};

#[derive(Deserialize)]
struct ScrapeRequest {
    url: String,
    // future proofing if user wants to expand features
    #[serde(default = "default_format")]
    format: String,
}

fn default_format() -> String {
    "json".to_string()
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

/// Shared application state with authenticated client.
struct AppState {
    api_key: String,
    client: FetchClient,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // The API key is defined in the Dokploy Environment Settings as API_KEY
    let api_key = env::var("API_KEY").unwrap_or_else(|_| {
        warn!("API_KEY environment variable not set! API is running without authentication.");
        "".to_string()
    });

    // Build fetch client with proxy support
    let mut config = FetchConfig::default();

    // Load single proxy from WEBCLAW_PROXY
    match env::var("WEBCLAW_PROXY") {
        Ok(proxy) => {
            info!("using single proxy from WEBCLAW_PROXY");
            config.proxy = Some(proxy);
        }
        Err(_) => {
            info!("WEBCLAW_PROXY not set - running without proxy");
        }
    }

    // Load proxy pool from WEBCLAW_PROXY_FILE (defaults to proxies.txt)
    let proxy_file = env::var("WEBCLAW_PROXY_FILE")
        .ok()
        .unwrap_or_else(|| "proxies.txt".to_string());
    if std::path::Path::new(&proxy_file).exists() {
        if let Ok(pool) = webclaw_fetch::parse_proxy_file(&proxy_file) && !pool.is_empty() {
            info!(count = pool.len(), file = %proxy_file, "loaded proxy pool");
            config.proxy_pool = pool;
        }
    }

    let client = match FetchClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("failed to initialize fetch client: {e}");
            std::process::exit(1);
        }
    };

    info!(proxy_pool_size = client.proxy_pool_size(), "fetch client initialized");

    let state = Arc::new(AppState { api_key, client });

    let app = Router::new()
        .route("/api/scrape", post(scrape_handler))
        .with_state(state);

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a number");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("webclaw-api listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn scrape_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ScrapeRequest>,
) -> impl IntoResponse {
    // Basic Authentication Check
    if !state.api_key.is_empty() {
        let auth_header = headers.get("authorization").and_then(|h| h.to_str().ok());
        let expected = format!("Bearer {}", state.api_key);
        if auth_header != Some(&expected) {
            let err = ApiError {
                error: "Unauthorized: Invalid API_KEY".to_string(),
            };
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::to_value(err).unwrap()),
            )
                .into_response();
        }
    }

    // Perform extraction
    let options = ExtractionOptions::default();
    match state.client.fetch_and_extract_with_options(&payload.url, &options).await {
        Ok(result) => {
            // Return JSON containing metadata, content, and the structured_data we fixed
            match serde_json::to_value(result) {
                Ok(val) => (StatusCode::OK, Json(val)).into_response(),
                Err(e) => {
                    let err = ApiError {
                        error: format!("Failed to serialize result: {}", e),
                    };
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::to_value(err).unwrap()),
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            let err = ApiError {
                error: format!("Extraction failed for {}: {}", payload.url, e),
            };
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::to_value(err).unwrap()),
            )
                .into_response()
        }
    }
}
