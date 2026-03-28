use async_trait::async_trait;

use crate::{config::Config, error::AppError, models::FoodItem};

pub mod claude;

pub struct AnalysisInput {
    pub image_bytes: Option<Vec<u8>>,
    pub image_mime: Option<String>,
    pub image_url: Option<String>,
    pub text: Option<String>,
}

#[async_trait]
pub trait AiEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn analyze(&self, input: AnalysisInput) -> Result<Vec<FoodItem>, AppError>;
}

pub fn build_engine(name: &str, config: &Config) -> Result<Box<dyn AiEngine>, AppError> {
    match name {
        "claude" => Ok(Box::new(claude::ClaudeEngine::new(&config.anthropic_api_key))),
        other => Err(AppError::UnknownEngine(other.to_string())),
    }
}
