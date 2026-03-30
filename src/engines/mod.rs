use async_trait::async_trait;

use crate::{config::Config, error::AppError, models::{ExtractionResult, FoodItem}};

pub mod claude;

#[derive(Clone)]
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

#[async_trait]
#[allow(dead_code)]
pub trait ExtractionEngine: Send + Sync {
    fn name(&self) -> &str;
    async fn extract(&self, input: AnalysisInput) -> Result<ExtractionResult, AppError>;
}

pub fn build_engine(name: &str, config: &Config) -> Result<Box<dyn AiEngine>, AppError> {
    match name {
        "claude" => Ok(Box::new(claude::ClaudeEngine::new(
            &config.anthropic_api_key,
            &config.ai_reasoning_model,
            &config.reasoning_prompt,
        ))),
        other => Err(AppError::UnknownEngine(other.to_string())),
    }
}

pub fn build_extraction_engine(name: &str, config: &Config) -> Result<Box<dyn ExtractionEngine>, AppError> {
    match name {
        "claude" => Ok(Box::new(claude::ClaudeExtractionEngine::new(
            &config.anthropic_api_key,
            &config.ai_extraction_model,
            &config.extraction_prompt,
        ))),
        other => Err(AppError::UnknownEngine(other.to_string())),
    }
}
