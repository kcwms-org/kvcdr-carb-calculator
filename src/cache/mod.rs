use std::time::Duration;

use image_hasher::{HashAlg, HasherConfig, ImageHash};
use moka::future::Cache;
use redis::AsyncCommands;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use crate::models::FoodItem;

const PHASH_HAMMING_THRESHOLD: u32 = 10;

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

    /// Build a cache key from engine name + text + image.
    /// For images, uses perceptual hash so near-identical images share a key.
    /// Falls back to SHA-256 of raw bytes if image cannot be decoded.
    pub fn cache_key(engine: &str, text: Option<&str>, image_bytes: Option<&[u8]>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(engine.as_bytes());

        if let Some(t) = text {
            hasher.update(t.trim().to_lowercase().as_bytes());
        }

        if let Some(img) = image_bytes {
            let phash = perceptual_hash(img);
            hasher.update(phash.as_bytes());
        }

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

/// Compute a perceptual hash string for image bytes.
/// Returns a canonical hex string that is the same for near-identical images
/// (Hamming distance ≤ PHASH_HAMMING_THRESHOLD).
///
/// Near-identical images are bucketed by quantising the hash: we zero out the
/// lowest bits so images within the threshold map to the same bucket key.
/// Falls back to SHA-256 hex of raw bytes if the image cannot be decoded.
fn perceptual_hash(image_bytes: &[u8]) -> String {
    let img = match image::load_from_memory(image_bytes) {
        Ok(img) => img,
        Err(_) => {
            let mut h = Sha256::new();
            h.update(image_bytes);
            return hex::encode(h.finalize());
        }
    };

    // Gradient pHash needs at least ~32x32 source pixels to produce meaningful
    // hashes; resize up if the image is smaller.
    let img = if img.width() < 32 || img.height() < 32 {
        img.resize(32, 32, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let hasher = HasherConfig::new()
        .hash_alg(HashAlg::Gradient)
        .hash_size(8, 8)
        .to_hasher();

    let hash: ImageHash = hasher.hash_image(&img);

    // Bucket by zeroing bits whose position < threshold so that hashes within
    // Hamming distance ≤ PHASH_HAMMING_THRESHOLD collapse to the same key.
    bucket_hash(&hash, PHASH_HAMMING_THRESHOLD)
}

/// Zero out the lowest `threshold` bits of the hash bytes so that hashes
/// differing by at most `threshold` bits land in the same bucket.
fn bucket_hash(hash: &ImageHash, threshold: u32) -> String {
    let mut bytes = hash.as_bytes().to_vec();
    let mut bits_to_zero = threshold;
    for byte in bytes.iter_mut().rev() {
        if bits_to_zero == 0 {
            break;
        }
        if bits_to_zero >= 8 {
            *byte = 0;
            bits_to_zero -= 8;
        } else {
            let mask = 0xFFu8 << bits_to_zero;
            *byte &= mask;
            bits_to_zero = 0;
        }
    }
    hex::encode(bytes)
}
