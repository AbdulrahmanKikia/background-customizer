use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Bundled still images shipped with the app (Milestone 3/4 samples).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundledSample {
    A,
    B,
}

impl BundledSample {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }

    pub fn from_index(index: usize) -> Self {
        if index.is_multiple_of(2) {
            Self::A
        } else {
            Self::B
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

/// Wallpaper selected for one monitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WallpaperSource {
    Bundled { sample: BundledSample },
    Path { path: String },
}

impl WallpaperSource {
    pub fn bundled(sample: BundledSample) -> Self {
        Self::Bundled { sample }
    }

    pub fn path(path: impl Into<String>) -> Result<Self, CoreError> {
        let path = path.into();
        if path.is_empty() {
            return Err(CoreError::Invalid("wallpaper path is empty".to_string()));
        }
        Ok(Self::Path { path })
    }
}

#[cfg(test)]
mod tests {
    use super::{BundledSample, WallpaperSource};

    #[test]
    fn bundled_sample_index_round_trips() {
        assert_eq!(BundledSample::from_index(0), BundledSample::A);
        assert_eq!(BundledSample::from_index(1), BundledSample::B);
        assert_eq!(BundledSample::A.index(), 0);
        assert_eq!(BundledSample::B.index(), 1);
    }

    #[test]
    fn path_wallpaper_rejects_empty() {
        assert!(WallpaperSource::path("").is_err());
        assert!(WallpaperSource::path("C:/wall.png").is_ok());
    }
}
