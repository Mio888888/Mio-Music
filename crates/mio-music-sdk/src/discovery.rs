//! Discovery categories and charts, as plugins describe them.
use crate::PlaylistItem;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryCategory {
    pub id: String,
    pub name: String,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryCategoryGroup {
    pub name: String,
    pub list: Vec<DiscoveryCategory>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryCategories {
    pub groups: Vec<DiscoveryCategoryGroup>,
    pub hot: Vec<DiscoveryCategory>,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryPlaylistPage {
    pub list: Vec<PlaylistItem>,
    pub page: usize,
    /// None when the source does not expose a total; never an estimated total.
    pub total: Option<i64>,
    pub has_more: bool,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryChart {
    /// Raw provider ID. Use `discovery_chart_detail`, not `playlist_detail`.
    pub id: String,
    pub name: String,
    pub img: String,
    pub source: String,
    pub update_frequency: String,
}
