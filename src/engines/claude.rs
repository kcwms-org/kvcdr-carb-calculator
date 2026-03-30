use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{json, Value};

use crate::{
    error::AppError,
    models::{ExtractionResult, FoodItem},
};

use super::{AiEngine, AnalysisInput, ExtractionEngine};

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";

fn build_message_content(input: &AnalysisInput) -> Vec<Value> {
    let mut content = Vec::new();

    if let Some(url) = &input.image_url {
        content.push(json!({
            "type": "image",
            "source": {
                "type": "url",
                "url": url
            }
        }));
    } else if let (Some(bytes), Some(mime)) = (&input.image_bytes, &input.image_mime) {
        let encoded = BASE64.encode(bytes);
        content.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": mime,
                "data": encoded
            }
        }));
    }

    let text = match &input.text {
        Some(t) if !t.trim().is_empty() => t.clone(),
        _ => "Please analyze the food in the image and estimate carbohydrates for each item.".to_string(),
    };

    content.push(json!({ "type": "text", "text": text }));
    content
}

// ── Reasoning engine ─────────────────────────────────────────────────────────

pub struct ClaudeEngine {
    api_key: String,
    model: String,
    system_prompt: String,
    client: reqwest::Client,
}

impl ClaudeEngine {
    pub fn new(api_key: &str, model: &str, system_prompt: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AiEngine for ClaudeEngine {
    fn name(&self) -> &str {
        &self.model
    }

    async fn analyze(&self, input: AnalysisInput) -> Result<Vec<FoodItem>, AppError> {
        if input.image_bytes.is_none() && input.image_url.is_none() && input.text.is_none() {
            return Err(AppError::InvalidRequest(
                "Either image or text input is required".to_string(),
            ));
        }

        let content = build_message_content(&input);

        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "system": self.system_prompt,
            "messages": [{ "role": "user", "content": content }]
        });

        let response = self
            .client
            .post(CLAUDE_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let response_text = response.text().await?;

        if !status.is_success() {
            return Err(AppError::EngineError(format!(
                "Claude API returned {}: {}",
                status, response_text
            )));
        }

        let api_response: Value = serde_json::from_str(&response_text)
            .map_err(|e| AppError::JsonParseError(format!("Failed to parse API response: {}", e)))?;

        let content_text = api_response["content"][0]["text"]
            .as_str()
            .ok_or_else(|| AppError::EngineError("No text content in Claude response".to_string()))?;

        let json_str = content_text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let items: Vec<FoodItem> = serde_json::from_str(json_str).map_err(|e| {
            AppError::JsonParseError(format!(
                "Failed to parse food items JSON: {}. Raw: {}",
                e, json_str
            ))
        })?;

        Ok(items)
    }
}

// ── Extraction engine ─────────────────────────────────────────────────────────

pub struct ClaudeExtractionEngine {
    api_key: String,
    model: String,
    system_prompt: String,
    client: reqwest::Client,
}

impl ClaudeExtractionEngine {
    pub fn new(api_key: &str, model: &str, system_prompt: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExtractionEngine for ClaudeExtractionEngine {
    fn name(&self) -> &str {
        &self.model
    }

    async fn extract(&self, input: AnalysisInput) -> Result<ExtractionResult, AppError> {
        if input.image_bytes.is_none() && input.image_url.is_none() && input.text.is_none() {
            return Err(AppError::InvalidRequest(
                "Either image or text input is required".to_string(),
            ));
        }

        let content = build_message_content(&input);

        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "system": self.system_prompt,
            "messages": [{ "role": "user", "content": content }]
        });

        let response = self
            .client
            .post(CLAUDE_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let response_text = response.text().await?;

        if !status.is_success() {
            return Err(AppError::EngineError(format!(
                "Claude API returned {}: {}",
                status, response_text
            )));
        }

        let api_response: Value = serde_json::from_str(&response_text)
            .map_err(|e| AppError::JsonParseError(format!("Failed to parse API response: {}", e)))?;

        let content_text = api_response["content"][0]["text"]
            .as_str()
            .ok_or_else(|| AppError::EngineError("No text content in Claude response".to_string()))?;

        let json_str = content_text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let result: ExtractionResult = serde_json::from_str(json_str).map_err(|e| {
            AppError::JsonParseError(format!(
                "Failed to parse ExtractionResult JSON: {}. Raw: {}",
                e, json_str
            ))
        })?;

        Ok(result)
    }
}
