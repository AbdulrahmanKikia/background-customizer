//! Still-image wallpaper library for the settings UI.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use core_lib::{BundledSample, WallpaperSource};
use decoder::{thumbnail_data_url_from_bytes, thumbnail_data_url_from_path};
use serde::{Deserialize, Serialize};

const SAMPLE_A_PNG: &[u8] = include_bytes!("../../../assets/sample-wallpaper.png");
const SAMPLE_B_PNG: &[u8] = include_bytes!("../../../assets/sample-wallpaper-b.png");
const THUMB_EDGE: u32 = 192;

fn bundled_thumb(sample: BundledSample) -> Result<&'static str, String> {
    static THUMB_A: OnceLock<String> = OnceLock::new();
    static THUMB_B: OnceLock<String> = OnceLock::new();
    let cell = match sample {
        BundledSample::A => &THUMB_A,
        BundledSample::B => &THUMB_B,
    };
    if let Some(existing) = cell.get() {
        return Ok(existing.as_str());
    }
    let bytes = match sample {
        BundledSample::A => SAMPLE_A_PNG,
        BundledSample::B => SAMPLE_B_PNG,
    };
    let encoded = thumbnail_data_url_from_bytes(bytes, THUMB_EDGE)?;
    Ok(cell.get_or_init(|| encoded).as_str())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryKind {
    Bundled,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItem {
    pub id: String,
    pub title: String,
    pub kind: LibraryKind,
    pub wallpaper: WallpaperSource,
    pub thumb_data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LibraryIndex {
    entries: Vec<StoredEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredEntry {
    id: String,
    title: String,
    path: String,
}

pub fn library_dir(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("library")
}

fn index_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("library.json")
}

fn load_index(config_path: &Path) -> Result<LibraryIndex, String> {
    let path = index_path(config_path);
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LibraryIndex::default()),
        Err(error) => Err(error.to_string()),
    }
}

fn save_index(config_path: &Path, index: &LibraryIndex) -> Result<(), String> {
    let path = index_path(config_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let raw = serde_json::to_string_pretty(index).map_err(|error| error.to_string())?;
    fs::write(path, raw).map_err(|error| error.to_string())
}

fn bundled_item(sample: BundledSample, title: &str) -> Result<LibraryItem, String> {
    Ok(LibraryItem {
        id: format!("bundled:{}", sample.as_str()),
        title: title.to_string(),
        kind: LibraryKind::Bundled,
        wallpaper: WallpaperSource::bundled(sample),
        thumb_data_url: bundled_thumb(sample)?.to_string(),
    })
}

fn thumb_cache_path(image_path: &Path) -> PathBuf {
    let mut os = image_path.as_os_str().to_owned();
    os.push(format!(".thumb{THUMB_EDGE}.txt"));
    PathBuf::from(os)
}

fn file_thumb(path: &Path) -> Result<String, String> {
    let cache = thumb_cache_path(path);
    if let Ok(existing) = fs::read_to_string(&cache) {
        if existing.starts_with("data:image/") {
            return Ok(existing);
        }
    }
    let encoded = thumbnail_data_url_from_path(path, THUMB_EDGE)?;
    let _ = fs::write(&cache, &encoded);
    Ok(encoded)
}

fn file_item(entry: &StoredEntry) -> Result<LibraryItem, String> {
    let path = Path::new(&entry.path);
    if !path.is_file() {
        return Err(format!("library file missing: {}", entry.path));
    }
    Ok(LibraryItem {
        id: entry.id.clone(),
        title: entry.title.clone(),
        kind: LibraryKind::File,
        wallpaper: WallpaperSource::path(&entry.path).map_err(|error| error.to_string())?,
        thumb_data_url: file_thumb(path)?,
    })
}

/// Bundled samples plus imported files from disk.
pub fn list_library(config_path: &Path) -> Result<Vec<LibraryItem>, String> {
    let mut items = vec![
        bundled_item(BundledSample::A, "Sample A — teal & coral")?,
        bundled_item(BundledSample::B, "Sample B — purple & gold")?,
    ];
    let index = load_index(config_path)?;
    for entry in &index.entries {
        match file_item(entry) {
            Ok(item) => items.push(item),
            Err(error) => eprintln!("skipping library entry {}: {error}", entry.id),
        }
    }
    Ok(items)
}

pub fn find_library_item(config_path: &Path, id: &str) -> Result<LibraryItem, String> {
    if let Some(sample) = id.strip_prefix("bundled:") {
        let sample = match sample {
            "a" => BundledSample::A,
            "b" => BundledSample::B,
            other => return Err(format!("unknown bundled sample: {other}")),
        };
        let title = match sample {
            BundledSample::A => "Sample A — teal & coral",
            BundledSample::B => "Sample B — purple & gold",
        };
        return bundled_item(sample, title);
    }
    let index = load_index(config_path)?;
    let entry = index
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| format!("unknown library item: {id}"))?;
    file_item(entry)
}

/// Copy an image into the app library and return its catalog entry.
pub fn import_image(config_path: &Path, source: &Path) -> Result<LibraryItem, String> {
    if !source.is_file() {
        return Err(format!("file not found: {}", source.display()));
    }
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" => {}
        other => return Err(format!("unsupported image type: .{other}")),
    }

    // Prove the file decodes before copying it in.
    let _ = thumbnail_data_url_from_path(source, THUMB_EDGE)?;

    let dir = library_dir(config_path);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let id = format!(
        "file:{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    );
    let file_name = format!("{}.{}", id.trim_start_matches("file:"), extension);
    let dest = dir.join(&file_name);
    fs::copy(source, &dest).map_err(|error| error.to_string())?;

    let title = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Imported image")
        .to_string();
    let entry = StoredEntry {
        id: id.clone(),
        title,
        path: dest.display().to_string(),
    };
    let mut index = load_index(config_path)?;
    index.entries.push(entry.clone());
    save_index(config_path, &index)?;
    file_item(&entry)
}

#[cfg(test)]
mod tests {
    use super::{import_image, list_library};
    use std::fs;

    #[test]
    fn library_lists_bundled_samples_and_imports() {
        let root = std::env::temp_dir().join(format!(
            "bc-library-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("dir");
        let config_path = root.join("config.json");

        let listed = list_library(&config_path).expect("list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, "bundled:a");
        assert_eq!(listed[1].id, "bundled:b");
        assert!(listed[0]
            .thumb_data_url
            .starts_with("data:image/png;base64,"));

        let source = root.join("import.png");
        fs::write(&source, super::SAMPLE_A_PNG).expect("write");
        let imported = import_image(&config_path, &source).expect("import");
        assert!(imported.id.starts_with("file:"));
        assert_eq!(imported.title, "import");

        let listed = list_library(&config_path).expect("list after");
        assert_eq!(listed.len(), 3);
        let _ = fs::remove_dir_all(&root);
    }
}
