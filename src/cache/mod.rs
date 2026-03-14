use std::time::Duration;

use moka::future::Cache;
use sha2::{Digest, Sha256};

use crate::models::FoodItem;

#[derive(Clone)]
pub struct AnalysisCache {
    inner: Cache<String, Vec<FoodItem>>,
}

impl AnalysisCache {
    pub fn new(ttl_secs: u64) -> Self {
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(ttl_secs))
            .max_capacity(1_000)
            .build();

        Self { inner: cache }
    }

    pub fn cache_key(engine: &str, text: Option<&str>, image_bytes: Option<&[u8]>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(engine.as_bytes());
        if let Some(t) = text {
            hasher.update(t.trim().to_lowercase().as_bytes());
        }
        if let Some(img) = image_bytes {
            hasher.update(img);
        }
        hex::encode(hasher.finalize())
    }

    pub async fn get(&self, key: &str) -> Option<Vec<FoodItem>> {
        self.inner.get(key).await
    }

    pub async fn set(&self, key: String, items: Vec<FoodItem>) {
        self.inner.insert(key, items).await;
    }
}
