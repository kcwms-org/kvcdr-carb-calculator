use std::sync::Arc;

use async_trait::async_trait;
use axum::{routing::post, Router};
use axum_test::TestServer;

use crate::{
    cache::AnalysisCache,
    engines::{AiEngine, AnalysisInput, ExtractionEngine},
    error::AppError,
    models::{ExtractedItem, ExtractionResult, FoodItem},
    routes::analyze::{analyze_handler, AppState},
};

// --- Mock engines ---

struct MockExtractionEngine {
    result: ExtractionResult,
}

impl MockExtractionEngine {
    fn returning(result: ExtractionResult) -> Arc<Self> {
        Arc::new(Self { result })
    }
}

#[async_trait]
impl ExtractionEngine for MockExtractionEngine {
    fn name(&self) -> &str {
        "mock-extraction"
    }

    async fn extract(&self, _input: AnalysisInput) -> Result<ExtractionResult, AppError> {
        Ok(self.result.clone())
    }
}

struct MockEngine {
    items: Vec<FoodItem>,
}

impl MockEngine {
    fn returning(items: Vec<FoodItem>) -> Arc<Self> {
        Arc::new(Self { items })
    }
}

#[async_trait]
impl AiEngine for MockEngine {
    fn name(&self) -> &str {
        "mock-reasoning"
    }

    async fn analyze(&self, _input: AnalysisInput) -> Result<Vec<FoodItem>, AppError> {
        Ok(self.items.clone())
    }
}

struct FailingEngine;

#[async_trait]
impl AiEngine for FailingEngine {
    fn name(&self) -> &str {
        "failing"
    }

    async fn analyze(&self, _input: AnalysisInput) -> Result<Vec<FoodItem>, AppError> {
        Err(AppError::EngineError("simulated engine failure".to_string()))
    }
}

// --- Helpers ---

fn test_items() -> Vec<FoodItem> {
    vec![
        FoodItem {
            name: "oatmeal".to_string(),
            carbs_grams: 27.0,
            confidence: Some("high".to_string()),
            notes: None,
        },
        FoodItem {
            name: "banana".to_string(),
            carbs_grams: 27.0,
            confidence: Some("high".to_string()),
            notes: None,
        },
    ]
}

fn test_extraction_result() -> ExtractionResult {
    ExtractionResult {
        version: "1".to_string(),
        items: vec![
            ExtractedItem {
                item: "oatmeal".to_string(),
                quantity: "1".to_string(),
                quantity_type: "cup".to_string(),
            },
            ExtractedItem {
                item: "banana".to_string(),
                quantity: "1".to_string(),
                quantity_type: "individual".to_string(),
            },
        ],
    }
}

fn make_server(
    extraction: Arc<dyn ExtractionEngine>,
    reasoning: Arc<dyn AiEngine>,
) -> TestServer {
    let state = AppState {
        extraction_engine: extraction,
        reasoning_engine: reasoning,
        cache: AnalysisCache::new(60, None),
        spaces: None,
    };
    let app = Router::new()
        .route("/analyze", post(analyze_handler))
        .with_state(state);
    TestServer::new(app).unwrap()
}

// --- Handler tests ---

#[tokio::test]
async fn text_only_returns_items_and_total() {
    let server = make_server(
        MockExtractionEngine::returning(test_extraction_result()),
        MockEngine::returning(test_items()),
    );

    let response = server
        .post("/analyze")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("text", "a bowl of oatmeal with banana"),
        )
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["total_carbs_grams"], 54.0);
    assert_eq!(body["engine_used"], "mock-reasoning");
    assert_eq!(body["cached"], false);
}

#[tokio::test]
async fn second_identical_request_is_cached() {
    let server = make_server(
        MockExtractionEngine::returning(test_extraction_result()),
        MockEngine::returning(test_items()),
    );

    let multipart = || {
        axum_test::multipart::MultipartForm::new()
            .add_text("text", "a bowl of oatmeal with banana")
    };

    let first = server.post("/analyze").multipart(multipart()).await;
    first.assert_status_ok();
    assert_eq!(first.json::<serde_json::Value>()["cached"], false);

    let second = server.post("/analyze").multipart(multipart()).await;
    second.assert_status_ok();
    assert_eq!(second.json::<serde_json::Value>()["cached"], true);
}

#[tokio::test]
async fn missing_input_returns_400() {
    let server = make_server(
        MockExtractionEngine::returning(test_extraction_result()),
        MockEngine::returning(vec![]),
    );

    // Whitespace-only text — handler rejects with 400
    let response = server
        .post("/analyze")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("text", "   "),
        )
        .await;

    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert!(body["error"].as_str().unwrap().contains("required"));
}

#[tokio::test]
async fn engine_error_returns_502() {
    let state = AppState {
        extraction_engine: MockExtractionEngine::returning(test_extraction_result()),
        reasoning_engine: Arc::new(FailingEngine),
        cache: AnalysisCache::new(60, None),
        spaces: None,
    };
    let app = Router::new()
        .route("/analyze", post(analyze_handler))
        .with_state(state);
    let server = TestServer::new(app).unwrap();

    let response = server
        .post("/analyze")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("text", "pizza"),
        )
        .await;

    response.assert_status(axum::http::StatusCode::BAD_GATEWAY);
    let body: serde_json::Value = response.json();
    assert!(body["error"].as_str().unwrap().contains("simulated engine failure"));
}

// --- Cache key unit tests ---

#[test]
fn cache_key_stable_for_same_items_different_order() {
    let items_a = vec![
        ExtractedItem { item: "bacon".to_string(), quantity: "3".to_string(), quantity_type: "strip".to_string() },
        ExtractedItem { item: "scrambled egg".to_string(), quantity: "2".to_string(), quantity_type: "individual".to_string() },
    ];
    let items_b = vec![
        ExtractedItem { item: "scrambled egg".to_string(), quantity: "2".to_string(), quantity_type: "individual".to_string() },
        ExtractedItem { item: "bacon".to_string(), quantity: "3".to_string(), quantity_type: "strip".to_string() },
    ];
    assert_eq!(
        AnalysisCache::cache_key("claude-sonnet-4-6", "1", &items_a),
        AnalysisCache::cache_key("claude-sonnet-4-6", "1", &items_b),
    );
}

#[test]
fn cache_key_differs_by_reasoning_model() {
    let items = vec![
        ExtractedItem { item: "scrambled egg".to_string(), quantity: "2".to_string(), quantity_type: "individual".to_string() },
    ];
    assert_ne!(
        AnalysisCache::cache_key("claude-sonnet-4-6", "1", &items),
        AnalysisCache::cache_key("claude-haiku-4-5", "1", &items),
    );
}

#[test]
fn cache_key_differs_by_extraction_version() {
    let items = vec![
        ExtractedItem { item: "scrambled egg".to_string(), quantity: "2".to_string(), quantity_type: "individual".to_string() },
    ];
    assert_ne!(
        AnalysisCache::cache_key("claude-sonnet-4-6", "1", &items),
        AnalysisCache::cache_key("claude-sonnet-4-6", "2", &items),
    );
}

// --- Moka fallback (no Redis configured) ---

#[tokio::test]
async fn moka_cache_works_without_redis() {
    let cache = AnalysisCache::new(60, None);
    let key = "test_key".to_string();

    assert!(cache.get(&key).await.is_none());

    cache.set(key.clone(), test_items()).await;

    let result = cache.get(&key).await;
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 2);
}
