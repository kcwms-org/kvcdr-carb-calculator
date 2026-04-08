# Plan: Add datetime input and return full-sized images from /analyze

**Status:** Done — merged via PR #30

## Context

The Android client wants to record *when* a meal was logged. The server needs to accept a datetime from the client, validate it, and echo it back in the response. Additionally, the client needs the original image(s) returned so it can display them locally without re-downloading from Spaces.

---

## Changes

### 1. Add `chrono` to `Cargo.toml`

`chrono` is not currently a dependency. Add it with serde support:

```toml
chrono = { version = "0.4", features = ["serde"] }
```

---

### 2. `src/models/mod.rs`

Add `datetime` and `images` to `AnalyzeResponse`:

```rust
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, ToSchema)]
pub struct ImageData {
    pub data: String,        // base64-encoded bytes
    pub mime_type: String,   // e.g. "image/jpeg"
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AnalyzeResponse {
    pub items: Vec<FoodItem>,
    pub total_carbs_grams: f32,
    pub engine_used: String,
    pub cached: bool,
    pub datetime: DateTime<Utc>,
    pub images: Vec<ImageData>,   // full-sized images provided in the request
}
```

---

### 3. `src/routes/analyze.rs`

#### Parse new `datetime` multipart field

Add to the local variable declarations:
```rust
let mut datetime_input: Option<String> = None;
```

Add a match arm:
```rust
Some("datetime") => {
    let value = field.text().await
        .map_err(|e| AppError::MultipartError(e.to_string()))?;
    if !value.trim().is_empty() {
        datetime_input = Some(value.trim().to_string());
    }
}
```

#### Validate datetime after multipart loop

After the multipart loop:

```rust
use chrono::{DateTime, Utc};

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
```

#### Collect images for response

After parsing all multipart fields, collect images:

```rust
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

let mut response_images: Vec<ImageData> = Vec::new();
if let (Some(bytes), Some(mime)) = (&image_bytes, &image_mime) {
    response_images.push(ImageData {
        data: BASE64.encode(bytes),
        mime_type: mime.clone(),
    });
}
```

Note: `image_url` inputs are not included — the client already has the URL. Only `image` (uploaded bytes) are base64-encoded and returned.

#### Include `datetime` and `images` in both response paths

Cache hit path:
```rust
return Ok(Json(AnalyzeResponse {
    items: cached_items,
    total_carbs_grams: total,
    engine_used: reasoning_model,
    cached: true,
    datetime,
    images: response_images,
}));
```

Normal path:
```rust
Ok(Json(AnalyzeResponse {
    items,
    total_carbs_grams: total,
    engine_used: reasoning_model,
    cached: false,
    datetime,
    images: response_images,
}))
```

#### Update `AnalyzeRequest` doc struct (OpenAPI only)

```rust
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct AnalyzeRequest {
    image: Option<Vec<u8>>,
    image_url: Option<String>,
    text: Option<String>,
    /// ISO 8601 / RFC 3339 datetime of the meal (e.g. 2026-04-08T12:00:00Z). Must not be in the future. Defaults to server time if omitted.
    datetime: Option<String>,
}
```

---

## Critical files

| File | Change |
|---|---|
| `Cargo.toml` | Add `chrono = { version = "0.4", features = ["serde"] }` |
| `src/models/mod.rs` | Add `ImageData` struct; extend `AnalyzeResponse` with `datetime` and `images` |
| `src/routes/analyze.rs` | Parse `datetime` field, validate ≤ now, collect image bytes, populate response |

No changes needed to engine files — `AnalysisInput` is unchanged. The datetime and images are purely request metadata that bypass the AI pipeline.

---

## Verification

```bash
# Build
cargo build

# Test with datetime and image upload
curl -X POST http://localhost:3000/analyze \
  -F "image=@test.jpg" \
  -F "text=bowl of oatmeal" \
  -F "datetime=2026-04-08T08:30:00Z"
# → response includes datetime, images[0].data (base64), images[0].mime_type

# Test future datetime rejected
curl -X POST http://localhost:3000/analyze \
  -F "text=oatmeal" \
  -F "datetime=2099-01-01T00:00:00Z"
# → 400 "datetime must not be in the future"

# Test invalid datetime rejected
curl -X POST http://localhost:3000/analyze \
  -F "text=oatmeal" \
  -F "datetime=not-a-date"
# → 400 "datetime must be a valid RFC 3339 timestamp"

# Test omitting datetime defaults to server time
curl -X POST http://localhost:3000/analyze \
  -F "text=oatmeal"
# → response includes datetime ≈ now

# Run tests
cargo test
```
