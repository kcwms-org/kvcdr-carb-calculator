use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedItem {
    pub item: String,
    pub quantity: String,
    pub quantity_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub version: String,
    /// Simplified restatement of the input produced by the extraction step. Logged only;
    /// not part of the cache key.
    #[serde(default)]
    pub normalized_text: Option<String>,
    pub items: Vec<ExtractedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodItem {
    pub name: String,
    pub carbs_grams: f32,
    pub confidence: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ImageData {
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnalyzeResponse {
    pub items: Vec<FoodItem>,
    pub total_carbs_grams: f32,
    pub engine_used: String,
    pub cached: bool,
    pub datetime: DateTime<Utc>,
    pub images: Vec<ImageData>,
}
