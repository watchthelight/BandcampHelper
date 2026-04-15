use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub username: String,
    pub collection_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionItem {
    pub sale_item_type: String,
    pub sale_item_id: u64,
    pub band_name: String,
    pub item_title: String,
    pub item_id: u64,
    pub item_url: String,
    pub redownload_url: String,
    pub purchased: Option<String>,
    pub item_art_id: Option<u64>,
    pub tralbum_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionResult {
    pub items: Vec<CollectionItem>,
    pub fan_id: u64,
    pub username: String,
    pub collection_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub items: Vec<CollectionItem>,
    pub format: String,
    pub output_dir: String,
    pub parallel: usize,
    pub extract: bool,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckLocalRequest {
    pub items: Vec<CollectionItem>,
    pub format: String,
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub album_key: String,
    pub artist: String,
    pub title: String,
    pub status: String,
    pub bytes_downloaded: u64,
    pub bytes_total: u64,
    pub file_path: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverallProgress {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub skipped: usize,
}
