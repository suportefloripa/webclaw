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

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // The API key is defined in the Dokploy Environment Settings as API_KEY
    let api_key = env::var("API_KEY").unwrap_or_else(|_| {
        tracing::warn!("API_KEY environment variable not set! API is running without authentication.");
        "".to_string()
    });

    let app = Router::new()
        .route("/api/scrape", post(scrape_handler))
        .with_state(api_key);

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a number");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("webclaw-api listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn scrape_handler(
    headers: HeaderMap,
    State(api_key): State<String>,
    Json(payload): Json<ScrapeRequest>,
) -> impl IntoResponse {
    // Basic Authentication Check
    if !api_key.is_empty() {
        let auth_header = headers.get("authorization").and_then(|h| h.to_str().ok());
        let expected = format!("Bearer {}", api_key);
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

    // Initialize the fetch client with default webclaw configuration
    let client = match FetchClient::new(FetchConfig::default()) {
        Ok(c) => c,
        Err(e) => {
            let err = ApiError {
                error: format!("Failed to initialize fetch client: {}", e),
            };
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(err).unwrap()),
            )
                .into_response();
        }
    };

    // Perform extraction
    let options = ExtractionOptions::default();
    match client.fetch_and_extract_with_options(&payload.url, &options).await {
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
