# Issue #31 — presign endpoint returns 403 on S3 upload

## Summary

The Android client receives HTTP 403 (`SignatureDoesNotMatch`) when uploading to DigitalOcean Spaces via presigned PUT URL. The `/presign` endpoint itself returns 200 successfully, but the subsequent PUT to the presigned URL fails because extra headers sent by the client (specifically `Content-Type: image/jpeg`) are not included in the presigned URL's signed header scope.

## Root cause / context

The root cause was diagnosed in the related client issue (`kcwms-org/kvcdr-carb-calculator-client#19`):

1. The backend generates a presigned PUT URL that signs only `host` and `x-amz-acl` headers (see `src/spaces.rs:56` — `.acl(ObjectCannedAcl::PublicRead)`).
2. The `/presign` response tells the client to include `x-amz-acl: public-read` via `required_headers`, but says nothing about `Content-Type`.
3. The Android client's OkHttp automatically injects `Content-Type: image/jpeg` when a media type is passed to `toRequestBody()`.
4. S3/Spaces recomputes the signature including the extra `Content-Type` header → mismatch → 403.

**Client-side fix** (already applied in the client repo): remove the media type argument from `toRequestBody()` so OkHttp doesn't inject an unsigned `Content-Type`.

**Backend hardening** (this issue): the presigned URL should include `Content-Type` in the signed headers so that clients can safely send it. The `required_headers` response should also include `Content-Type: image/jpeg` (or a configurable type) to guide clients.

## Proposed approach

### Option A — Sign `Content-Type` into the presigned URL (recommended)

1. **`src/spaces.rs`** — Add `.content_type("image/jpeg")` to the `put_object()` builder before `.presigned()`. This includes `Content-Type` in the signed headers, so clients sending `Content-Type: image/jpeg` will match the signature.

2. **`src/routes/presign.rs`** — Add `Content-Type: image/jpeg` to the `required_headers` map so the client knows to include it.

3. **Tests** — Add/update tests to verify the presigned URL includes `content-type` in `X-Amz-SignedHeaders` and that `required_headers` contains both `x-amz-acl` and `Content-Type`.

### Future consideration

If we need to support multiple image types (PNG, HEIC, etc.), the `/presign` endpoint could accept an optional `content_type` query parameter. For now, `image/jpeg` covers the primary use case (phone camera photos).

## Risks & open questions

- **DigitalOcean Spaces compatibility**: Verify that DO Spaces handles `content_type` on presigned PUT the same way as AWS S3. The `aws-sdk-s3` crate should handle this, but worth a manual test.
- **Client coordination**: The client-side fix (removing media type from `toRequestBody()`) is already merged. After this backend fix, the client could optionally re-add the media type since it will now be in the signed scope. Not strictly necessary — both approaches work after this fix.
- **Existing presigned URLs**: Any presigned URLs generated before this fix (within the 5-minute TTL) will still fail if the client sends `Content-Type`. This is a non-issue since the TTL is short.

## Acceptance criteria

- [ ] Presigned PUT URL includes `content-type` in `X-Amz-SignedHeaders`
- [ ] `required_headers` in `/presign` response includes `Content-Type: image/jpeg`
- [ ] Client can upload with `Content-Type: image/jpeg` header without 403
- [ ] Existing tests pass; new test covers the signed headers
- [ ] Manual test: full upload flow (presign → PUT → analyze → delete) succeeds
