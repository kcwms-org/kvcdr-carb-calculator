use std::collections::HashMap;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{error::AppError, routes::analyze::AppState};

#[derive(Serialize, ToSchema)]
pub struct PresignResponse {
    /// Presigned PUT URL — client uploads the image directly to this URL (expires in 5 minutes)
    pub upload_url: String,
    /// Public URL of the image after upload — pass this as `image_url` to POST /analyze
    pub image_url: String,
    /// Object key — pass this to DELETE /upload/{key} after analysis to clean up
    pub key: String,
    /// Headers the client MUST include in the PUT request to upload_url
    pub required_headers: HashMap<String, String>,
}

/// Get a presigned PUT URL for uploading an image directly to object storage.
///
/// Workflow:
/// 1. `GET /presign` — get `upload_url`, `image_url`, and `key`
/// 2. `PUT {upload_url}` — upload the image bytes directly (Content-Type: image/jpeg etc.)
/// 3. `POST /analyze` with `image_url` — analyze the image
/// 4. `DELETE /upload/{key}` — delete the temporary object
#[utoipa::path(
    get,
    path = "/presign",
    responses(
        (status = 200, description = "Presigned upload URL", body = PresignResponse),
        (status = 503, description = "Object storage not configured"),
    ),
    tag = "upload"
)]
pub async fn presign_handler(
    State(state): State<AppState>,
) -> Result<Json<PresignResponse>, AppError> {
    let spaces = state.spaces.as_ref().ok_or_else(|| {
        AppError::SpacesError("Object storage is not configured on this server".to_string())
    })?;

    let (upload_url, image_url, key) = spaces.presign_put().await?;

    let mut required_headers = HashMap::new();
    required_headers.insert("x-amz-acl".to_string(), "public-read".to_string());
    required_headers.insert("Content-Type".to_string(), "image/jpeg".to_string());

    Ok(Json(PresignResponse {
        upload_url,
        image_url,
        key,
        required_headers,
    }))
}

/// Delete a temporary upload object from object storage.
#[utoipa::path(
    delete,
    path = "/upload/{key}",
    params(("key" = String, Path, description = "Object key returned by GET /presign")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 503, description = "Object storage not configured"),
    ),
    tag = "upload"
)]
pub async fn delete_upload_handler(
    State(state): State<AppState>,
    axum::extract::Path(key): axum::extract::Path<String>,
) -> Result<axum::http::StatusCode, AppError> {
    let spaces = state.spaces.as_ref().ok_or_else(|| {
        AppError::SpacesError("Object storage is not configured on this server".to_string())
    })?;

    spaces.delete(&key).await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
