mod cache;
mod config;
mod engines;
mod error;
mod models;
mod routes;

use std::sync::Arc;

use axum::{extract::DefaultBodyLimit, routing::{get, post}, Router};
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use cache::AnalysisCache;
use config::Config;
use engines::{build_engine, build_extraction_engine};
use routes::analyze::{analyze_handler, AppState};

#[derive(OpenApi)]
#[openapi(
    paths(routes::analyze::analyze_handler),
    components(schemas(models::FoodItem, models::AnalyzeResponse, routes::analyze::AnalyzeRequest)),
    tags(
        (name = "analyze", description = "Carbohydrate analysis endpoints"),
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
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kvcdr_carb_calculator=debug,info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let cache = AnalysisCache::new(config.cache_ttl_secs, config.redis_url.as_deref());
    let extraction_engine = Arc::from(build_extraction_engine(&config.default_engine, &config)?);
    let reasoning_engine = Arc::from(build_engine(&config.default_engine, &config)?);

    let state = AppState { extraction_engine, reasoning_engine, cache };

    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi()))
        .route("/health", get(|| async { "OK" }))
        .route("/analyze", post(analyze_handler))
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024)) // 20 MB ceiling; clients should resize to <1 MB to fit App Platform ingress
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.server_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on {}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
