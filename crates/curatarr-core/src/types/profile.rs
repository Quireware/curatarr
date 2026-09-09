use serde::{Deserialize, Serialize};

use super::enums::FileFormat;
use super::id::QualityProfileId;

pub const DEFAULT_QUALITY_PROFILE_ID: &str = "00000000-0000-7000-8000-000000000001";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityProfile {
    pub id: QualityProfileId,
    pub name: String,
    pub format_order: Vec<FileFormat>,
    pub max_size_bytes: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_id_parses() {
        DEFAULT_QUALITY_PROFILE_ID
            .parse::<QualityProfileId>()
            .unwrap();
    }

    #[test]
    fn format_order_serde() {
        let json = serde_json::to_string(&vec![FileFormat::Epub, FileFormat::Pdf]).unwrap();
        let back: Vec<FileFormat> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, vec![FileFormat::Epub, FileFormat::Pdf]);
    }
}
