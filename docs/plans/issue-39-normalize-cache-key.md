# Issue #39 — Similar text queries should return cached results

**Status:** Done — merged via PR #40

## Summary

Two text submissions describing the same food a minute apart ("Half a cup of dried oatmeal"
and ".5 cups oatmeal") both returned `"cached": false`, even though they produced the same
27 g result. Equivalent descriptions should map to the same cache key so the second request
skips the phase-2 reasoning call.

## Root cause / context

The cache key (`AnalysisCache::cache_key` in `src/cache/mod.rs`) is a SHA-256 of the reasoning
model, the extraction schema version, and the phase-1 `ExtractedItem`s serialized as JSON
(sorted by `item`). The JSON is hashed verbatim, so any difference in the extraction output
makes a different key:

- **Name drift** — the extraction model (Haiku) does not reliably produce one canonical name.
  The returned item names ("Dried oatmeal (rolled oats, uncooked)" vs "Oatmeal (dry rolled oats,
  0.5 cup)") suggest extraction produced something like `"dried oatmeal"` vs `"oatmeal"`.
- **Quantity drift** — `quantity` is a free-form string. `"0.5"`, `".5"`, `"1/2"`, `"0.50"`
  and `"half"` all hash differently.
- **Unit drift** — `quantity_type` is free-form in practice (`"cup"` vs `"cups"`, `"Cup"`).

We don't log the extracted items today, so we can't confirm which field differed for these two
requests. The fix should handle all three.

## Proposed approach

0. **Make extraction deterministic.** `call_claude` in `src/engines/claude.rs` sends no
   `temperature`, so phase 1 runs at the API default (1.0) and can phrase the same food
   differently on every call. Set `temperature: 0` for the extraction call (keep the reasoning
   call as is, or make it configurable). This is likely the largest single win.

1. **Log extracted items.** In `src/routes/analyze.rs`, add the extracted items (and the
   first 8 chars of the cache key) to the `phase 1: extraction complete` log line. This makes
   future cache misses easy to diagnose from App Platform logs.

2. **Add a normalization module** — `src/cache/normalize.rs` with
   `pub fn normalize_item(item: &ExtractedItem) -> ExtractedItem`, applied to every item in
   `cache_key()` before sorting and hashing:
   - **Name:** lowercase, trim, collapse whitespace, drop parenthetical text, strip
     punctuation, drop filler modifiers that don't change carbs (`dried`, `dry`, `uncooked`,
     `raw`, `plain`, `fresh`), and singularize a trailing plural `s` (careful with words like
     `hummus`, `asparagus` — keep a small exception list).
   - **Quantity:** parse to `f64` from decimals (`".5"`, `"0.50"`), fractions (`"1/2"`,
     `"1 1/2"`), and words (`"half"`, `"quarter"`, `"one"`…`"ten"`); format with up to 3
     decimals, trimming trailing zeros (`0.5`). Fall back to the trimmed lowercase string if it
     can't be parsed.
   - **Unit:** lowercase, trim, map synonyms/plurals to the canonical set from
     `prompts/extraction.txt` (`cups`→`cup`, `tablespoon(s)`/`tbsps`→`tbsp`,
     `ounce(s)`→`oz`, `slices`→`slice`, `pieces`→`piece`, `strips`→`strip`,
     `each`/`whole`/`pcs`→ as appropriate).
   - Sort by the normalized name, then quantity, then unit, so the key is stable even when two
     items share a name.

3. **Add a simplify-then-extract step to the extraction prompt** (`prompts/extraction.txt`).
   Requested in an issue comment: have the existing extraction engine first reduce the input to
   simple nouns and food measurements, then do its current work. This happens in the same Claude
   call (no new engine, no extra latency):
   - **Step A — simplify.** Rewrite the description as a plain list of `<number> <unit> <food>`
     phrases. Drop articles, filler adverbs and adjectives that don't change carbs
     ("a", "the", "about", "some", "delicious", "dried", "uncooked" for oats, "fresh", "plain").
     Turn number words and fractions into decimals ("half a cup" → `0.5 cup`).
   - **Keep carb-relevant modifiers.** Not every adjective is filler. Keep the ones that change
     carbs per serving: preparation state where it matters (`cooked` vs `uncooked` rice and
     pasta), variety (`brown rice`, `whole wheat bread`, `sweet potato`), and sugar content
     (`sugar-free`, `diet`, `sweetened`). Turn size adjectives into amounts instead of dropping
     them ("a large apple" → `1 large apple` stays as quantity + size, or a weight in `g`).
     List these keep/drop rules with examples in the prompt.
   - **Restrict units** to common food-preparation measurements: `tsp`, `tbsp`, `cup`,
     `fl oz`, `ml`, `l`, `oz`, `lb`, `g`, `kg`, plus count units (`individual`, `slice`, `piece`,
     `strip`). Extend the unit mapping in step 2 to cover this full set (`teaspoon(s)` → `tsp`,
     `liter(s)`/`litre(s)` → `l`, `pound(s)`/`lbs` → `lb`, `gram(s)` → `g`, and so on).
   - **Step B — extract** `{item, quantity, quantity_type}` from the simplified phrases, as now.
   - Return the simplified text in a new optional `normalized_text` field in the extraction
     JSON so it can be logged (step 1). It is not part of the cache key; the key is still built
     from `items`. Add `#[serde(default)] normalized_text: Option<String>` to
     `ExtractionResult` in `src/models/mod.rs`.
   - Require `quantity` as a decimal number (`"0.5"`, never `".5"` or `"1/2"` or `"half"`).
   - Add examples, including both inputs from this issue mapping to
     `0.5 cup oatmeal` → `{"item":"oatmeal","quantity":"0.5","quantity_type":"cup"}`.
   - Bump `"version"` to `"2"` in the schema example so old cache entries are not mixed with new
     keys. (The version string is part of the key, so this invalidates the cache once — fine,
     the TTL is 24 h anyway.)

4. **Tests** (in `src/routes/tests.rs`, or a new `#[cfg(test)]` block in
   `src/cache/normalize.rs`):
   - `cache_key` is equal for `{"dried oatmeal","0.5","cup"}` and `{"oatmeal",".5","cups"}`.
   - Quantity parsing: `".5"`, `"0.50"`, `"1/2"`, `"half"` → `"0.5"`; `"1 1/2"` → `"1.5"`.
   - Unit mapping for each synonym.
   - Different foods or amounts still produce different keys (`oatmeal 0.5 cup` ≠
     `oatmeal 1 cup`; `oatmeal` ≠ `rice`).
   - The existing `cache_key_stable_for_same_items_different_order` test still passes.

5. **Docs** — update the cache description in `CLAUDE.md` / README to say the key is built
   from normalized extracted items.

## Alternatives considered

Raised in an issue comment: use a word-normalization library, or add an engine plugin that
normalizes every query into a standard format first.

- **Separate normalization engine (extra LLM call before extraction):** not recommended. Phase 1
  extraction already is that engine: it turns free text or an image into
  `{item, quantity, quantity_type}`, and that output is what gets hashed. A second LLM call would
  add latency and cost, and would be just as nondeterministic unless run at temperature 0, in
  which case fixing phase 1 directly does the same job.
- **Normalization library on the raw query text:** not recommended as the main fix. Hashing
  happens on the extraction output, not the raw text, and image requests have no text at all.
  Generic text tools (stemmers, lemmatizers) also can't turn "half a cup" into `0.5 cup`; that
  needs the LLM or the small quantity parser in step 2.
- **Library on extracted names (partial use):** reasonable. A stemmer such as the
  `rust-stemmers` crate (Snowball English) could replace the hand-written plural rules in step 2
  for item names. Caveat: stems aren't words (`oatmeal` stays, but `berries` → `berri`), which is
  fine for a hash key but makes logs harder to read. The implementer can pick either; a stemmer
  is the lower-maintenance choice.

- **Part-of-speech stripping in Rust code (before the Claude call):** not recommended. Rust has
  no light, accurate part-of-speech tagger (the options pull in large ML models), it wouldn't
  help photo requests, and a blind "drop all adjectives" rule removes carb-relevant words like
  `brown`, `sweet`, `cooked` and `sugar-free`. Doing the simplification inside the extraction
  prompt (step 3) gets the same result with context-aware judgment.

## Risks & open questions

- **Over-normalization.** Dropping modifiers can merge foods with different carbs per serving
  — "cooked rice" vs "uncooked rice" differ ~3x per cup. Keep the modifier strip list to words
  that are clearly safe, and do **not** strip `cooked`. Consider whether "dried oatmeal" vs
  "cooked oatmeal" should really share a key; they should not. Prefer dropping only
  `dried`/`dry`/`uncooked` when the base food is one that's normally measured dry (oats), or
  just leave this to the prompt and keep code-level name normalization to formatting only.
  Implementer should pick the conservative option if unsure.
- **Nondeterministic extraction** can still produce different names (`"oatmeal"` vs
  `"rolled oat"`). Normalization reduces misses but won't remove them; an embedding or
  synonym-table approach is out of scope.
- Confirm `temperature: 0` does not hurt extraction quality on images (spot-check a few photos).
- The old cache entries become unreachable after the version bump; this is intended.

## Acceptance criteria

- The two inputs from the issue produce the same cache key, given the extraction outputs they
  likely produce (covered by a unit test on normalized items).
- Submitting "Half a cup of dried oatmeal" then ".5 cups oatmeal" against a running server
  returns `"cached": true` on the second request (manual check; depends on extraction output).
- Different foods and different amounts still get different keys.
- Extracted items are visible in the `phase 1: extraction complete` log line.
- `cargo build`, `cargo clippy -- -D warnings`, and `cargo test` pass.
