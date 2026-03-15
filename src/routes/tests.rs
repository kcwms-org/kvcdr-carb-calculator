use std::sync::Arc;

use async_trait::async_trait;
use axum::{routing::post, Router};
use axum_test::TestServer;

use crate::{
    cache::AnalysisCache,
    engines::{AiEngine, AnalysisInput},
    error::AppError,
    models::FoodItem,
    routes::analyze::{analyze_handler, AppState},
};

// --- Mock engine ---

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
        "mock"
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

fn make_server(engine: Arc<dyn AiEngine>) -> TestServer {
    let state = AppState {
        engine,
        cache: AnalysisCache::new(60),
    };
    let app = Router::new()
        .route("/analyze", post(analyze_handler))
        .with_state(state);
    TestServer::new(app).unwrap()
}

// --- Tests ---

#[tokio::test]
async fn text_only_returns_items_and_total() {
    let server = make_server(MockEngine::returning(test_items()));

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
    assert_eq!(body["engine_used"], "mock");
    assert_eq!(body["cached"], false);
}

#[tokio::test]
async fn second_identical_request_is_cached() {
    let server = make_server(MockEngine::returning(test_items()));

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
    let server = make_server(MockEngine::returning(vec![]));

    // Send whitespace-only text — handler should reject with 400
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
        engine: Arc::new(FailingEngine),
        cache: AnalysisCache::new(60),
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

#[tokio::test]
async fn cache_key_is_case_and_whitespace_insensitive() {
    let key1 = AnalysisCache::cache_key("claude", Some("  Oatmeal  "), None);
    let key2 = AnalysisCache::cache_key("claude", Some("oatmeal"), None);
    assert_eq!(key1, key2);
}

#[tokio::test]
async fn cache_key_differs_by_engine() {
    let key1 = AnalysisCache::cache_key("claude", Some("oatmeal"), None);
    let key2 = AnalysisCache::cache_key("other", Some("oatmeal"), None);
    assert_ne!(key1, key2);
}

#[tokio::test]
async fn cache_key_differs_by_image_bytes() {
    let key1 = AnalysisCache::cache_key("claude", None, Some(&[1, 2, 3]));
    let key2 = AnalysisCache::cache_key("claude", None, Some(&[4, 5, 6]));
    assert_ne!(key1, key2);
}
