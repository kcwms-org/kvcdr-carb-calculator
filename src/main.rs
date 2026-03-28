mod cache;
mod config;
mod engines;
mod error;
mod models;
mod routes;
mod spaces;

use std::sync::Arc;

use axum::{extract::DefaultBodyLimit, routing::{delete, get, post}, Router};
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use cache::AnalysisCache;
use config::Config;
use engines::build_engine;
use routes::analyze::{analyze_handler, AppState};
use routes::presign::{delete_upload_handler, presign_handler};
use spaces::SpacesClient;

#[derive(OpenApi)]
#[openapi(
    paths(routes::analyze::analyze_handler, routes::presign::presign_handler, routes::presign::delete_upload_handler),
    components(schemas(models::FoodItem, models::AnalyzeResponse, routes::analyze::AnalyzeRequest, routes::presign::PresignResponse)),
    tags(
        (name = "analyze", description = "Carbohydrate analysis endpoints"),
        (name = "upload", description = "Direct-to-storage image upload endpoints"),
    ),
    info(
        title = "kvcdr-carb-calculator",
        version = "0.1.0",
        description = "Estimates carbohydrates per food item using Claude vision AI"
    )
)]
struct ApiDoc;

fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    doc.info.version = format!("{}+{}", doc.info.version, env!("GIT_HASH"));
    doc
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kvcdr_carb_calculator=debug,info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let cache = AnalysisCache::new(config.cache_ttl_secs, config.redis_url.as_deref());
    let engine = Arc::from(build_engine(&config.default_engine, &config)?);

    let spaces = match (&config.spaces_key, &config.spaces_secret) {
        (Some(key), Some(secret)) => {
            info!("Spaces upload enabled (bucket: {})", config.spaces_bucket);
            Some(SpacesClient::new(key, secret, &config.spaces_region, &config.spaces_bucket))
        }
        _ => {
            info!("Spaces credentials not set — direct image upload only");
            None
        }
    };

    let state = AppState { engine, cache, spaces };

    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi()))
        .route("/health", get(|| async { "OK" }))
        .route("/presign", get(presign_handler))
        .route("/upload/{key}", delete(delete_upload_handler))
        .route("/analyze", post(analyze_handler))
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024)) // 20 MB — phone camera photos can exceed the 2 MB default
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.server_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on {}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
