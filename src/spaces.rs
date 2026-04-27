use std::time::Duration;

use aws_sdk_s3::{
    config::{Credentials, Region},
    presigning::PresigningConfig,
    Client,
};
use uuid::Uuid;

use crate::error::AppError;

const PRESIGN_TTL_SECS: u64 = 300; // 5 minutes — enough for a mobile upload

#[derive(Clone)]
pub struct SpacesClient {
    client: Client,
    bucket: String,
    public_base_url: String,
}

impl SpacesClient {
    pub fn new(access_key: &str, secret_key: &str, region: &str, bucket: &str) -> Self {
        let credentials = Credentials::new(access_key, secret_key, None, None, "spaces");
        let endpoint = format!("https://{}.digitaloceanspaces.com", region);

        let config = aws_sdk_s3::Config::builder()
            .credentials_provider(credentials)
            .region(Region::new(region.to_string()))
            .endpoint_url(&endpoint)
            .force_path_style(false)
            .build();

        Self {
            client: Client::from_conf(config),
            bucket: bucket.to_string(),
            public_base_url: format!("https://{}.{}.digitaloceanspaces.com", bucket, region),
        }
    }

    /// Generate a presigned PUT URL the client can use to upload an image directly to Spaces.
    /// Returns `(presigned_put_url, public_image_url, object_key)`.
    /// The caller must include `x-amz-acl: public-read` in the PUT request headers.
    pub async fn presign_put(&self) -> Result<(String, String, String), AppError> {
        let key = format!("tmp/{}", Uuid::new_v4());

        let presigning_config = PresigningConfig::builder()
            .expires_in(Duration::from_secs(PRESIGN_TTL_SECS))
            .build()
            .map_err(|e| AppError::SpacesError(e.to_string()))?;

        let presigned = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .acl(aws_sdk_s3::types::ObjectCannedAcl::PublicRead)
            .presigned(presigning_config)
            .await
            .map_err(|e| AppError::SpacesError(e.to_string()))?;

        let upload_url = presigned.uri().to_string();
        let image_url = format!("{}/{}", self.public_base_url, key);

        Ok((upload_url, image_url, key))
    }

    /// Delete an object by key after the client is done with it.
    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AppError::SpacesError(e.to_string()))?;
        Ok(())
    }
}
