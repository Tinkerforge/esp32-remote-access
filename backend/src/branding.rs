/// Branding configuration for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Brand {
    #[default]
    Warp,
    Seb,
}

impl Brand {
    /// Parse brand from environment variable or string
    pub fn from_env() -> Self {
        use std::str::FromStr;

        std::env::var("BRAND")
            .ok()
            .and_then(|s| Self::from_str(&s).ok())
            .unwrap_or(Brand::Warp)
    }
}

impl std::str::FromStr for Brand {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "seb" => Ok(Brand::Seb),
            "warp" => Ok(Brand::Warp),
            _ => Err(()),
        }
    }
}
