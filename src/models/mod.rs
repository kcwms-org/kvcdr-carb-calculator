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
    pub items: Vec<ExtractedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FoodItem {
    pub name: String,
    pub carbs_grams: f32,
    pub confidence: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnalyzeResponse {
    pub items: Vec<FoodItem>,
    pub total_carbs_grams: f32,
    pub engine_used: String,
    pub cached: bool,
}
