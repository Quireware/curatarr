use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_priority")]
    pub priority: u32,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_rate")]
    pub rate_limit_per_minute: u32,
}

fn default_true() -> bool {
    true
}

fn default_priority() -> u32 {
    50
}

fn default_rate() -> u32 {
    20
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataConfig {
    #[serde(default = "default_threshold")]
    pub match_threshold: f64,
    /// 0 disables the background refresh loop.
    #[serde(default)]
    pub refresh_interval_hours: u32,
    #[serde(default = "default_providers")]
    pub providers: Vec<ProviderConfig>,
}

fn default_threshold() -> f64 {
    0.6
}

fn default_providers() -> Vec<ProviderConfig> {
    vec![
        provider("openlibrary", true, 10, None, 20),
        provider("googlebooks", true, 20, None, 40),
        provider("anilist", true, 10, None, 30),
        provider("mangadex", true, 15, None, 20),
        provider("mangaupdates", true, 25, None, 20),
        provider("comicvine", false, 10, None, 200),
        provider("hardcover", false, 15, None, 20),
        provider("isbndb", false, 30, None, 20),
        provider("myanimelist", false, 20, None, 20),
        provider("librarything", false, 40, None, 10),
        provider("goodreads", false, 99, None, 1),
    ]
}

fn provider(
    name: &str,
    enabled: bool,
    priority: u32,
    api_key: Option<String>,
    rate_limit_per_minute: u32,
) -> ProviderConfig {
    ProviderConfig {
        name: name.to_string(),
        enabled,
        priority,
        api_key,
        rate_limit_per_minute,
    }
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            match_threshold: default_threshold(),
            refresh_interval_hours: 0,
            providers: default_providers(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        provider("openlibrary", true, 10, None, 20)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_keyless_providers() {
        let cfg = MetadataConfig::default();
        let names: Vec<_> = cfg
            .providers
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.name.as_str())
            .collect();
        assert!(names.contains(&"openlibrary"));
        assert!(names.contains(&"anilist"));
        assert!(names.contains(&"mangadex"));
        assert!(!names.contains(&"comicvine"));
    }
}
