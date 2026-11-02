use std::time::{Duration, Instant};

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

/// Timeouts for calls to the Anthropic API. Keeps a stuck call from outliving the
/// platform's request timeout, so the client gets a clear error instead of a bare 504.
#[derive(Clone, Copy, Debug)]
pub struct ClaudeTimeouts {
    pub connect: Duration,
    pub request: Duration,
}

fn build_client(timeouts: ClaudeTimeouts) -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .connect_timeout(timeouts.connect)
        .timeout(timeouts.request)
        .build()
        .map_err(AppError::from)
}

/// Sends one Messages API request and returns the text of the first content block.
/// Logs the duration and outcome of every call.
async fn call_claude(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    content: Vec<Value>,
    phase: &'static str,
) -> Result<String, AppError> {
    let body = json!({
        "model": model,
        "max_tokens": 1024,
        "system": system_prompt,
        "messages": [{ "role": "user", "content": content }]
    });

    let started = Instant::now();
    let result = async {
        let response = client
            .post(CLAUDE_API_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = response.text().await?;
        Ok::<_, reqwest::Error>((status, request_id, text))
    }
    .await;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let (status, request_id, response_text) = match result {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                phase,
                model,
                elapsed_ms,
                timeout = e.is_timeout(),
                connect = e.is_connect(),
                error = %e,
                "claude api call failed"
            );
            return Err(AppError::from(e));
        }
    };

    if !status.is_success() {
        tracing::error!(
            phase,
            model,
            elapsed_ms,
            status = status.as_u16(),
            request_id = %request_id,
            body = %response_text,
            "claude api returned an error"
        );
        return Err(AppError::EngineError(format!(
            "Claude API returned {}: {}",
            status, response_text
        )));
    }

    tracing::info!(
        phase,
        model,
        elapsed_ms,
        status = status.as_u16(),
        request_id = %request_id,
        "claude api call complete"
    );

    let api_response: Value = serde_json::from_str(&response_text)
        .map_err(|e| AppError::JsonParseError(format!("Failed to parse API response: {}", e)))?;

    let content_text = api_response["content"][0]["text"]
        .as_str()
        .ok_or_else(|| AppError::EngineError("No text content in Claude response".to_string()))?;

    Ok(content_text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string())
}

// ── Reasoning engine ─────────────────────────────────────────────────────────

pub struct ClaudeEngine {
    api_key: String,
    model: String,
    system_prompt: String,
    client: reqwest::Client,
}

impl ClaudeEngine {
    pub fn new(
        api_key: &str,
        model: &str,
        system_prompt: &str,
        timeouts: ClaudeTimeouts,
    ) -> Result<Self, AppError> {
        Ok(Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            client: build_client(timeouts)?,
        })
    }
}

#[async_trait]
impl AiEngine for ClaudeEngine {
    fn name(&self) -> &str {
        &self.model
    }

    async fn analyze(&self, input: AnalysisInput) -> Result<Vec<FoodItem>, AppError> {
        if input.image_bytes.is_none() && input.text.is_none() {
            return Err(AppError::InvalidRequest(
                "Either image or text input is required".to_string(),
            ));
        }

        let json_str = call_claude(
            &self.client,
            &self.api_key,
            &self.model,
            &self.system_prompt,
            build_message_content(&input),
            "reasoning",
        )
        .await?;

        let items: Vec<FoodItem> = serde_json::from_str(&json_str).map_err(|e| {
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
    pub fn new(
        api_key: &str,
        model: &str,
        system_prompt: &str,
        timeouts: ClaudeTimeouts,
    ) -> Result<Self, AppError> {
        Ok(Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            client: build_client(timeouts)?,
        })
    }
}

#[async_trait]
impl ExtractionEngine for ClaudeExtractionEngine {
    fn name(&self) -> &str {
        &self.model
    }

    async fn extract(&self, input: AnalysisInput) -> Result<ExtractionResult, AppError> {
        if input.image_bytes.is_none() && input.text.is_none() {
            return Err(AppError::InvalidRequest(
                "Either image or text input is required".to_string(),
            ));
        }

        let json_str = call_claude(
            &self.client,
            &self.api_key,
            &self.model,
            &self.system_prompt,
            build_message_content(&input),
            "extraction",
        )
        .await?;

        let result: ExtractionResult = serde_json::from_str(&json_str).map_err(|e| {
            AppError::JsonParseError(format!(
                "Failed to parse ExtractionResult JSON: {}. Raw: {}",
                e, json_str
            ))
        })?;

        Ok(result)
    }
}
