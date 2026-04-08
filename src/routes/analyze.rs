use std::sync::Arc;

use axum::{
    extract::{Multipart, State},
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use utoipa::ToSchema;

use crate::{
    cache::AnalysisCache,
    engines::{AiEngine, AnalysisInput, ExtractionEngine},
    error::AppError,
    models::{AnalyzeResponse, ImageData},
    spaces::SpacesClient,
};

#[derive(Clone)]
pub struct AppState {
    pub extraction_engine: Arc<dyn ExtractionEngine>,
    pub reasoning_engine: Arc<dyn AiEngine>,
    pub cache: AnalysisCache,
    pub spaces: Option<SpacesClient>,
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
    /// ISO 8601 / RFC 3339 datetime of the meal (e.g. 2026-04-08T12:00:00Z).
    /// Must not be in the future. Defaults to server time if omitted.
    datetime: Option<String>,
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
    tracing::info!("analyze request received");
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_mime: Option<String> = None;
    let mut image_url: Option<String> = None;
    let mut text: Option<String> = None;
    let mut datetime_input: Option<String> = None;

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
            Some("datetime") => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| AppError::MultipartError(e.to_string()))?;
                if !value.trim().is_empty() {
                    datetime_input = Some(value.trim().to_string());
                }
            }
            _ => {}
        }
    }

    if image_bytes.is_none() && image_url.is_none() && text.is_none() {
        tracing::warn!("request rejected: no image or text provided");
        return Err(AppError::InvalidRequest(
            "Either 'image' or 'text' field is required".to_string(),
        ));
    }

    // Parse and validate datetime
    let datetime: DateTime<Utc> = match datetime_input {
        Some(s) => {
            let parsed = DateTime::parse_from_rfc3339(&s)
                .map_err(|_| AppError::InvalidRequest(
                    "datetime must be a valid RFC 3339 timestamp (e.g. 2026-04-08T12:00:00Z)".to_string()
                ))?
                .with_timezone(&Utc);
            if parsed > Utc::now() {
                return Err(AppError::InvalidRequest(
                    "datetime must not be in the future".to_string()
                ));
            }
            parsed
        }
        None => Utc::now(),
    };

    // Collect images for response (only uploaded bytes, not URLs)
    let mut response_images: Vec<ImageData> = Vec::new();
    if let (Some(bytes), Some(mime)) = (&image_bytes, &image_mime) {
        response_images.push(ImageData {
            data: BASE64.encode(bytes),
            mime_type: mime.clone(),
        });
    }

    tracing::info!(
        has_image = image_bytes.is_some(),
        has_image_url = image_url.is_some(),
        has_text = text.is_some(),
        image_bytes = image_bytes.as_ref().map(|b| b.len()).unwrap_or(0),
        "input parsed"
    );

    // Phase 1: extraction — identify food items from image/text
    let extraction_input = AnalysisInput {
        image_bytes: image_bytes.clone(),
        image_mime: image_mime.clone(),
        image_url: image_url.clone(),
        text: text.clone(),
    };
    tracing::info!("phase 1: extraction start");
    let extraction_result = state.extraction_engine.extract(extraction_input).await?;
    tracing::info!(items = extraction_result.items.len(), "phase 1: extraction complete");

    // Cache key derived from normalized extraction output
    let reasoning_model = state.reasoning_engine.name().to_string();
    let cache_key = AnalysisCache::cache_key(
        &reasoning_model,
        &extraction_result.version,
        &extraction_result.items,
    );

    if let Some(cached_items) = state.cache.get(&cache_key).await {
        tracing::info!("cache hit — skipping phase 2");
        let total = cached_items.iter().map(|i| i.carbs_grams).sum();
        return Ok(Json(AnalyzeResponse {
            items: cached_items,
            total_carbs_grams: total,
            engine_used: reasoning_model,
            cached: true,
            datetime,
            images: response_images,
        }));
    }

    // Phase 2: reasoning — estimate carbs per item
    tracing::info!("phase 2: reasoning start");
    let reasoning_input = AnalysisInput {
        image_bytes,
        image_mime,
        image_url,
        text,
    };
    let items = state.reasoning_engine.analyze(reasoning_input).await?;
    tracing::info!(items = items.len(), "phase 2: reasoning complete");
    let total = items.iter().map(|i| i.carbs_grams).sum();

    state.cache.set(cache_key, items.clone()).await;

    Ok(Json(AnalyzeResponse {
        items,
        total_carbs_grams: total,
        engine_used: reasoning_model,
        cached: false,
        datetime,
        images: response_images,
    }))
}
