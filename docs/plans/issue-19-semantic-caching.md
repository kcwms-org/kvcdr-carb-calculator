# Plan: Semantic/AI-engine-level caching (Issue #19)

**Status:** Done — merged via PR #21
## Overview

Replace the current single-phase analysis with a two-phase pipeline:

1. **Extraction phase** — cheap model (Haiku) recognizes food items from image/text and returns a normalized `RecognitionDto` list
2. **Reasoning phase** — expensive model (Sonnet) estimates carb values per item

The cache key is derived from the sorted, normalized `RecognitionDto` list produced in Phase 1. Two very different descriptions of the same meal (e.g. `"all star breakfast with eggs"` vs `"Waffle House All Star with scrambled eggs and bacon"`) will resolve to the same item list and share a cache key — skipping the expensive reasoning call.

---

## Extraction Structs

Rust-idiomatic naming. Schema version is controlled manually and bumped with each PR that changes the shape.

```rust
pub struct ExtractedItem {
    pub item: String,
    pub quantity: String,
    pub quantity_type: String,
}

pub struct ExtractionResult {
    pub version: String,
    pub items: Vec<ExtractedItem>,
}
```

JSON representation returned by the extraction model:

```json
{
  "version": "1",
  "items": [
    { "item": "scrambled eggs", "quantity": "2", "quantity_type": "individual" },
    { "item": "bacon", "quantity": "3", "quantity_type": "strip" },
    { "item": "cheese grits", "quantity": "1", "quantity_type": "cup" }
  ]
}
```

- `item` — canonical food name (string, lowercase, singular)
- `quantity` — amount as a string (e.g. `"2"`, `"0.5"`, `"1/4"`)
- `quantity_type` — unit as a string (e.g. `"individual"`, `"oz"`, `"cup"`, `"tbsp"`, `"slice"`, `"strip"`)
- `version` — schema version string, e.g. `"1"`; bump when shape changes

Determinism is enforced by prompt quality, not post-processing. The extraction prompt instructs the model to use consistent lowercase singular names and standard unit strings.

---

## Cache Key Strategy

1. Take the `RecognitionDto` items list from Phase 1
2. Sort items alphabetically by `item` name
3. Serialize to a canonical JSON string (sorted keys, no whitespace)
4. Prepend `version` and `reasoning_model` name
5. SHA-256 hash the result

This means the cache key is stable across:
- Different wordings of the same meal
- Word order variations
- Minor description differences the extraction model normalizes away

Cache is **bypassed** (no lookup, no write) when:
- Input is image-only or `image_url`-only with no text **and** extraction returns no items (i.e. model cannot identify anything without visual context being passed through)
- Actually: extraction always runs — the cache key is always derived from Phase 1 output regardless of input type. Image-only inputs go through extraction too.

---

## Model Configuration

Two separate environment variables, both falling back to `DEFAULT_ENGINE` if unset:

```
AI_EXTRACTION_MODEL=claude-haiku-4-5-20251001
AI_REASONING_MODEL=claude-sonnet-4-6
DEFAULT_ENGINE=claude-sonnet-4-6   # fallback for both if above are absent
```

These are added to `Config` and the DO App Platform dashboard. `DEFAULT_ENGINE` is retained for backwards compatibility and as the fallback for both phases.

---

## Prompt Configuration

Prompts live as plain text files in the repo:

```
prompts/
├── extraction.txt   # Instructs model to return RecognitionDto JSON
└── reasoning.txt    # Instructs model to estimate carbs per item
```

Loaded at startup and stored in `Config` (or passed directly to the engine). Changing a prompt = edit the file + PR. Schema version bump required if the prompt change alters the DTO shape.

---

## Architecture Changes

### New: `ExtractionEngine` (or extend `AiEngine` trait)

Two options:
- **Option A:** Add a second trait method `extract()` to `AiEngine` alongside `analyze()`
- **Option B:** Keep `AiEngine` as-is, introduce a separate `ExtractionEngine` trait

Option B is cleaner — extraction and reasoning are distinct responsibilities. A `ClaudeExtractionEngine` and `ClaudeReasoningEngine` can share HTTP client infrastructure but have separate prompt/model config.

### Updated pipeline in `analyze_handler`

```
1. Run extraction (Phase 1) → Vec<RecognitionItem>
2. Compute cache key from sorted items
3. Cache hit  → return cached AnalyzeResponse (cached: true)
4. Cache miss → run reasoning (Phase 2) → Vec<FoodItem>
5. Store result under cache key
6. Return AnalyzeResponse (cached: false)
```

### `AnalysisCache::cache_key()` change

New signature:
```rust
pub fn cache_key(reasoning_model: &str, extraction_version: &str, items: &[ExtractedItem]) -> String
```

Removes all image/text inputs — key is derived solely from the normalized extraction output.

---

## Files to Change / Create

| File | Change |
|------|--------|
| `src/models/mod.rs` | Add `ExtractedItem`, `ExtractionResult` structs |
| `src/engines/mod.rs` | Add `ExtractionEngine` trait; update `AnalysisInput` if needed |
| `src/engines/claude.rs` | Implement extraction phase using Haiku model + extraction prompt |
| `src/cache/mod.rs` | Update `cache_key()` to take `RecognitionItem` list; remove perceptual hashing |
| `src/routes/analyze.rs` | Update handler to two-phase pipeline |
| `src/config.rs` | Add `ai_extraction_model`, `ai_reasoning_model`, prompt file paths |
| `src/main.rs` | Load prompt files at startup; wire new config fields |
| `prompts/extraction.txt` | New — extraction prompt (returns `RecognitionDto` JSON) |
| `prompts/reasoning.txt` | New — reasoning prompt (estimates carbs per item) |
| `Cargo.toml` | Remove `image_hasher`; check if `image` crate still needed |
| `src/routes/tests.rs` | Update cache key tests; add two-phase pipeline tests |
| `.env.example` | Add `AI_EXTRACTION_MODEL`, `AI_REASONING_MODEL` |
| `CLAUDE.md` | Update endpoint docs and env var table |

---

## Out of Scope

- Post-processing normalization of extraction output — prompt quality is the guard
- Redis configuration changes
- Vector/embedding-based similarity search
- Client repo changes
