/// Branding configuration for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Brand {
    Warp,
    Seb,
}

impl Brand {
    /// Parse brand from environment variable or string
    pub fn from_env() -> Self {
        std::env::var("BRAND")
            .ok()
            .and_then(|s| Self::from_str(&s))
            .unwrap_or(Brand::Warp)
    }

    /// Parse brand from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "seb" => Some(Brand::Seb),
            "warp" => Some(Brand::Warp),
            _ => None,
        }
    }
}

impl Default for Brand {
    fn default() -> Self {
        Brand::Warp
    }
}
