use std::sync::Arc;

use axum::{
    extract::{Multipart, State},
    Json,
};
use utoipa::ToSchema;

use crate::{
    cache::AnalysisCache,
    engines::{AiEngine, AnalysisInput},
    error::AppError,
    models::AnalyzeResponse,
};

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<dyn AiEngine>,
    pub cache: AnalysisCache,
}

/// Multipart form fields for /analyze
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct AnalyzeRequest {
    /// Food image file (JPEG, PNG, GIF, or WebP). Use this for small images only.
    /// For large phone camera photos, upload to DO Spaces first and supply `image_url` instead.
    #[schema(format = Binary, value_type = String)]
    image: Option<Vec<u8>>,
    /// Public URL of a pre-uploaded image (e.g. DigitalOcean Spaces).
    /// Preferred over `image` for large files to avoid platform upload limits.
    image_url: Option<String>,
    /// Text description of the food
    text: Option<String>,
}

/// Estimate carbohydrates from a food image or text description.
#[utoipa::path(
    post,
    path = "/analyze",
    request_body(
        content = AnalyzeRequest,
        content_type = "multipart/form-data"
    ),
    responses(
        (status = 200, description = "Carb breakdown per food item", body = AnalyzeResponse),
        (status = 400, description = "Missing or invalid input"),
        (status = 502, description = "AI engine error"),
    ),
    tag = "analyze"
)]
pub async fn analyze_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<AnalyzeResponse>, AppError> {
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_mime: Option<String> = None;
    let mut image_url: Option<String> = None;
    let mut text: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("length limit") || msg.contains("too large") || msg.contains("bytes") {
                AppError::MultipartError("Image exceeds the maximum upload size of 20 MB".to_string())
            } else {
                AppError::MultipartError(msg)
            }
        })?
    {
        match field.name() {
            Some("image") => {
                let content_type = field
                    .content_type()
                    .map(|ct| ct.to_string())
                    .unwrap_or_else(|| "image/jpeg".to_string());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        if msg.contains("length limit") || msg.contains("too large") || msg.contains("bytes") {
                            AppError::MultipartError("Image exceeds the maximum upload size of 20 MB".to_string())
                        } else {
                            AppError::MultipartError(msg)
                        }
                    })?;
                if !bytes.is_empty() {
                    image_mime = Some(content_type);
                    image_bytes = Some(bytes.to_vec());
                }
            }
            Some("image_url") => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| AppError::MultipartError(e.to_string()))?;
                if !value.trim().is_empty() {
                    image_url = Some(value.trim().to_string());
                }
            }
            Some("text") => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| AppError::MultipartError(e.to_string()))?;
                if !value.trim().is_empty() {
                    text = Some(value);
                }
            }
            _ => {}
        }
    }

    if image_bytes.is_none() && image_url.is_none() && text.is_none() {
        return Err(AppError::InvalidRequest(
            "Either 'image' or 'text' field is required".to_string(),
        ));
    }

    let engine_name = state.engine.name().to_string();

    let cache_key = AnalysisCache::cache_key(
        &engine_name,
        text.as_deref(),
        image_bytes.as_deref(),
    );

    if let Some(cached_items) = state.cache.get(&cache_key).await {
        let total = cached_items.iter().map(|i| i.carbs_grams).sum();
        return Ok(Json(AnalyzeResponse {
            items: cached_items,
            total_carbs_grams: total,
            engine_used: engine_name,
            cached: true,
        }));
    }

    let input = AnalysisInput {
        image_bytes,
        image_mime,
        image_url,
        text,
    };

    let items = state.engine.analyze(input).await?;
    let total = items.iter().map(|i| i.carbs_grams).sum();

    state.cache.set(cache_key, items.clone()).await;

    Ok(Json(AnalyzeResponse {
        items,
        total_carbs_grams: total,
        engine_used: engine_name,
        cached: false,
    }))
}
