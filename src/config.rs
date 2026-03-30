use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub anthropic_api_key: String,
    pub default_engine: String,
    pub ai_extraction_model: String,
    pub ai_reasoning_model: String,
    pub extraction_prompt: String,
    pub reasoning_prompt: String,
    pub cache_ttl_secs: u64,
    pub server_port: u16,
    pub redis_url: Option<String>,
    pub spaces_key: Option<String>,
    pub spaces_secret: Option<String>,
    pub spaces_region: String,
    pub spaces_bucket: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let default_engine = env::var("DEFAULT_ENGINE").unwrap_or_else(|_| "claude".to_string());

        Ok(Self {
            anthropic_api_key: env::var("ANTHROPIC_API_KEY")
                .context("ANTHROPIC_API_KEY must be set")?,
            ai_extraction_model: env::var("AI_EXTRACTION_MODEL")
                .unwrap_or_else(|_| default_engine.clone()),
            ai_reasoning_model: env::var("AI_REASONING_MODEL")
                .unwrap_or_else(|_| default_engine.clone()),
            extraction_prompt: std::fs::read_to_string("prompts/extraction.txt")
                .context("Failed to read prompts/extraction.txt")?,
            reasoning_prompt: std::fs::read_to_string("prompts/reasoning.txt")
                .context("Failed to read prompts/reasoning.txt")?,
            default_engine,
            cache_ttl_secs: env::var("CACHE_TTL_SECS")
                .unwrap_or_else(|_| "86400".to_string())
                .parse()
                .context("CACHE_TTL_SECS must be a valid integer")?,
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .context("SERVER_PORT must be a valid port number")?,
            redis_url: env::var("REDIS_URL").ok(),
            spaces_key: env::var("SPACES_ACCESS_KEY").ok(),
            spaces_secret: env::var("SPACES_SECRET_KEY").ok(),
            spaces_region: env::var("SPACES_REGION").unwrap_or_else(|_| "nyc3".to_string()),
            spaces_bucket: env::var("SPACES_BUCKET").unwrap_or_else(|_| "s3-kvcdr".to_string()),
        })
    }
}
