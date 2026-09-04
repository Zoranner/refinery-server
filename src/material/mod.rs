mod extraction;
mod model;

pub use extraction::normalize_reader_result;
pub(crate) use extraction::{download_only_response, normalize_reader_result_with_options};
pub use model::{
    Diagnostics, DownloadCapability, ExtractionResult, MaterialContentResponse, MaterialStatus,
    Pagination, TargetFacts,
};
