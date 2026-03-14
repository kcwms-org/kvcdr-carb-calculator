use axum::{
    extract::{Multipart, State},
    Json,
};

use crate::{
    cache::AnalysisCache,
    config::Config,
    engines::{build_engine, AnalysisInput},
    error::AppError,
    models::AnalyzeResponse,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub cache: AnalysisCache,
}

pub async fn analyze_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<AnalyzeResponse>, AppError> {
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_mime: Option<String> = None;
    let mut text: Option<String> = None;
    let mut engine_name: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::MultipartError(e.to_string()))?
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
                    .map_err(|e| AppError::MultipartError(e.to_string()))?;
                if !bytes.is_empty() {
                    image_mime = Some(content_type);
                    image_bytes = Some(bytes.to_vec());
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
            Some("engine") => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| AppError::MultipartError(e.to_string()))?;
                if !value.trim().is_empty() {
                    engine_name = Some(value);
                }
            }
            _ => {}
        }
    }

    if image_bytes.is_none() && text.is_none() {
        return Err(AppError::InvalidRequest(
            "Either 'image' or 'text' field is required".to_string(),
        ));
    }

    let engine_name = engine_name.unwrap_or_else(|| state.config.default_engine.clone());

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

    let engine = build_engine(&engine_name, &state.config)?;

    let input = AnalysisInput {
        image_bytes,
        image_mime,
        text,
    };

    let items = engine.analyze(input).await?;
    let total = items.iter().map(|i| i.carbs_grams).sum();

    state.cache.set(cache_key, items.clone()).await;

    Ok(Json(AnalyzeResponse {
        items,
        total_carbs_grams: total,
        engine_used: engine_name,
        cached: false,
    }))
}
