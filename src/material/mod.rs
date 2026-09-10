pub(crate) mod download;
mod extraction;
mod model;

pub use extraction::normalize_reader_result;
pub use model::{
    Diagnostics, DownloadCapability, ExtractionResult, MaterialContentResponse, MaterialStatus,
    Pagination, TargetFacts,
};
