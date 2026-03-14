use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoodItem {
    pub name: String,
    pub carbs_grams: f32,
    pub confidence: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub items: Vec<FoodItem>,
    pub total_carbs_grams: f32,
    pub engine_used: String,
    pub cached: bool,
}
