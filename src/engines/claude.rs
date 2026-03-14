use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{json, Value};

use crate::{error::AppError, models::FoodItem};

use super::{AiEngine, AnalysisInput};

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const CLAUDE_MODEL: &str = "claude-sonnet-4-5";

const SYSTEM_PROMPT: &str = r#"You are a nutrition analysis assistant specializing in carbohydrate estimation.

When given a food image and/or description:
1. Identify each distinct food item visible or mentioned
2. Estimate the carbohydrate content in grams per item based on a typical serving size
3. Assign a confidence level: "high" (well-known food with reliable data), "medium" (estimated from similar foods), or "low" (highly uncertain)
4. Add brief notes if relevant (e.g., "assumes 1 cup serving", "estimate based on visual portion")

Return ONLY a valid JSON array matching this exact schema — no markdown, no explanation:
[
  {
    "name": "food item name",
    "carbs_grams": 27.0,
    "confidence": "high",
    "notes": "optional note or null"
  }
]"#;

pub struct ClaudeEngine {
    api_key: String,
    client: reqwest::Client,
}

impl ClaudeEngine {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            client: reqwest::Client::new(),
        }
    }

    fn build_message_content(input: &AnalysisInput) -> Vec<Value> {
        let mut content = Vec::new();

        if let (Some(bytes), Some(mime)) = (&input.image_bytes, &input.image_mime) {
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
}

#[async_trait]
impl AiEngine for ClaudeEngine {
    fn name(&self) -> &str {
        "claude"
    }

    async fn analyze(&self, input: AnalysisInput) -> Result<Vec<FoodItem>, AppError> {
        if input.image_bytes.is_none() && input.text.is_none() {
            return Err(AppError::InvalidRequest(
                "Either image or text input is required".to_string(),
            ));
        }

        let content = Self::build_message_content(&input);

        let body = json!({
            "model": CLAUDE_MODEL,
            "max_tokens": 1024,
            "system": SYSTEM_PROMPT,
            "messages": [
                { "role": "user", "content": content }
            ]
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

        // Strip markdown code fences if present
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
