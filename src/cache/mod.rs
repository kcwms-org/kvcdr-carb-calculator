use std::time::Duration;

use moka::future::Cache;
use redis::AsyncCommands;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use crate::models::{ExtractedItem, FoodItem};

#[derive(Clone)]
pub struct AnalysisCache {
    moka: Cache<String, Vec<FoodItem>>,
    redis: Option<redis::Client>,
    ttl_secs: u64,
}

impl AnalysisCache {
    pub fn new(ttl_secs: u64, redis_url: Option<&str>) -> Self {
        let moka = Cache::builder()
            .time_to_live(Duration::from_secs(ttl_secs))
            .max_capacity(1_000)
            .build();

        let redis = redis_url.and_then(|url| {
            match redis::Client::open(url) {
                Ok(client) => {
                    debug!("Redis cache enabled at {}", url);
                    Some(client)
                }
                Err(e) => {
                    warn!("Failed to connect to Redis, falling back to moka-only: {}", e);
                    None
                }
            }
        });

        Self { moka, redis, ttl_secs }
    }

    /// Build a cache key from reasoning model name, extraction schema version,
    /// and the sorted list of extracted items. Items are sorted by name so that
    /// the same meal described in different word order produces the same key.
    pub fn cache_key(
        reasoning_model: &str,
        extraction_version: &str,
        items: &[ExtractedItem],
    ) -> String {
        let mut sorted: Vec<&ExtractedItem> = items.iter().collect();
        sorted.sort_by(|a, b| a.item.cmp(&b.item));

        let canonical = serde_json::to_string(&sorted).unwrap_or_default();

        let mut hasher = Sha256::new();
        hasher.update(reasoning_model.as_bytes());
        hasher.update(extraction_version.as_bytes());
        hasher.update(canonical.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub async fn get(&self, key: &str) -> Option<Vec<FoodItem>> {
        // 1. Check moka (fastest)
        if let Some(items) = self.moka.get(key).await {
            debug!("moka cache hit for key {}", &key[..8]);
            return Some(items);
        }

        // 2. Check Redis
        if let Some(client) = &self.redis {
            match client.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    let result: redis::RedisResult<Option<String>> = conn.get(key).await;
                    match result {
                        Ok(Some(json)) => {
                            if let Ok(items) = serde_json::from_str::<Vec<FoodItem>>(&json) {
                                debug!("Redis cache hit for key {}", &key[..8]);
                                // Backfill moka
                                self.moka.insert(key.to_string(), items.clone()).await;
                                return Some(items);
                            }
                        }
                        Ok(None) => {}
                        Err(e) => warn!("Redis get error: {}", e),
                    }
                }
                Err(e) => warn!("Redis connection error: {}", e),
            }
        }

        None
    }

    pub async fn set(&self, key: String, items: Vec<FoodItem>) {
        // Write to moka
        self.moka.insert(key.clone(), items.clone()).await;

        // Write to Redis
        if let Some(client) = &self.redis {
            match client.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    if let Ok(json) = serde_json::to_string(&items) {
                        let result: redis::RedisResult<()> =
                            conn.set_ex(&key, json, self.ttl_secs).await;
                        if let Err(e) = result {
                            warn!("Redis set error: {}", e);
                        } else {
                            debug!("Redis cache set for key {}", &key[..8]);
                        }
                    }
                }
                Err(e) => warn!("Redis connection error: {}", e),
            }
        }
    }
}
