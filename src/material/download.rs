use std::{future::Future, time::Duration};

use crate::error::ApiError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DownloadStage {
    Connect,
    Redirect,
    Body,
}

#[derive(Debug)]
pub(crate) struct DownloadError {
    pub(crate) stage: DownloadStage,
    error: ApiError,
}

impl DownloadError {
    pub(crate) fn new(stage: DownloadStage, error: ApiError) -> Self {
        Self { stage, error }
    }

    pub(crate) fn into_api_error(self) -> ApiError {
        match self.stage {
            DownloadStage::Connect | DownloadStage::Redirect | DownloadStage::Body => self.error,
        }
    }

    pub(crate) fn timeout(stage: DownloadStage) -> Self {
        Self::new(stage, ApiError::fetch_timeout())
    }
}

pub(crate) async fn within_budget<F, T>(
    future: F,
    timeout: Duration,
    stage: DownloadStage,
) -> Result<T, DownloadError>
where
    F: Future<Output = Result<T, DownloadError>>,
{
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| DownloadError::timeout(stage))?
}

#[cfg(test)]
mod tests {
    use super::{DownloadStage, within_budget};
    use std::time::Duration;

    #[tokio::test]
    async fn timeout_keeps_the_download_stage_for_internal_diagnostics() {
        let error = within_budget(
            async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok::<_, super::DownloadError>(())
            },
            Duration::from_millis(1),
            DownloadStage::Body,
        )
        .await
        .expect_err("the fixture must exceed the short body budget");

        assert_eq!(error.stage, DownloadStage::Body);
    }
}
