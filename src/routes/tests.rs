use std::sync::Arc;

use async_trait::async_trait;
use axum::{routing::post, Router};
use axum_test::TestServer;
use image::{ImageBuffer, Rgb};

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
        cache: AnalysisCache::new(60, None),
    };
    let app = Router::new()
        .route("/analyze", post(analyze_handler))
        .with_state(state);
    TestServer::new(app).unwrap()
}

/// Encode a 32x32 pixel grid as PNG bytes.
fn encode_png_32(pixels: &[[u8; 3]; 1024]) -> Vec<u8> {
    let img = ImageBuffer::from_fn(32, 32, |x, y| Rgb(pixels[(y * 32 + x) as usize]));
    let mut buf = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Png,
    )
    .unwrap();
    buf
}

// --- Handler tests ---

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
        engine: Arc::new(FailingEngine),
        cache: AnalysisCache::new(60, None),
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
async fn cache_key_differs_for_very_different_images() {
    // Horizontal gradient vs vertical gradient — maximally different pHashes
    let mut h_grad = [[0u8; 3]; 1024];
    let mut v_grad = [[0u8; 3]; 1024];
    for i in 0..1024usize {
        let hv = ((i % 32) * 8) as u8;
        let vv = ((i / 32) * 8) as u8;
        h_grad[i] = [hv, hv, hv];
        v_grad[i] = [vv, vv, vv];
    }

    let key1 = AnalysisCache::cache_key("claude", None, Some(&encode_png_32(&h_grad)));
    let key2 = AnalysisCache::cache_key("claude", None, Some(&encode_png_32(&v_grad)));
    assert_ne!(key1, key2, "structurally different images should have different cache keys");
}

#[tokio::test]
async fn cache_key_same_for_near_identical_images() {
    // Two images differing by a single pixel value should share a cache key
    let mut base = [[200u8; 3]; 1024];
    let mut tweaked = base;
    tweaked[0] = [201, 200, 200]; // 1-pixel difference

    // Ensure the base has a gradient so pHash is non-zero
    for i in 0..1024usize {
        let v = ((i % 32) * 8) as u8;
        base[i] = [v, v, v];
        tweaked[i] = [v, v, v];
    }
    tweaked[0] = [base[0][0].saturating_add(1), base[0][1], base[0][2]];

    let key1 = AnalysisCache::cache_key("claude", None, Some(&encode_png_32(&base)));
    let key2 = AnalysisCache::cache_key("claude", None, Some(&encode_png_32(&tweaked)));
    assert_eq!(key1, key2, "near-identical images should share a cache key");
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
