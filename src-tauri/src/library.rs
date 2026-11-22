use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "webm", "mov", "m4v", "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus",
];
const TEMPORARY_SUFFIXES: &[&str] = &[
    ".part",
    ".ytdl",
    ".tmp",
    ".temp",
    ".download",
    ".crdownload",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub size: u64,
    pub modified_at: i64,
}

pub fn scan_library(save_dir: &Path) -> Result<Vec<LibraryFile>, String> {
    if !save_dir.exists() {
        return Ok(Vec::new());
    }
    let root = save_dir
        .canonicalize()
        .map_err(|e| format!("无法访问媒体保存目录：{e}"))?;
    if !root.is_dir() {
        return Err("媒体保存路径不是目录。".into());
    }

    let mut directories = vec![root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) if directory == root => {
                return Err(format!("读取媒体目录 {} 失败：{e}", directory.display()));
            }
            Err(_) => continue,
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                directories.push(path);
                continue;
            }
            if !file_type.is_file() || !is_supported_media_file(&path) {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) if metadata.is_file() => metadata,
                _ => continue,
            };
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            files.push(LibraryFile {
                path: path.to_string_lossy().into_owned(),
                name: entry.file_name().to_string_lossy().into_owned(),
                extension,
                size: metadata.len(),
                modified_at: metadata.modified().map(system_time_ms).unwrap_or(0),
            });
        }
    }

    files.sort_by(|a, b| {
        b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(files)
}

pub fn validate_library_file(save_dir: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = save_dir
        .canonicalize()
        .map_err(|e| format!("无法访问媒体保存目录：{e}"))?;
    if !root.is_dir() {
        return Err("媒体保存路径不是目录。".into());
    }
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let link_metadata = fs::symlink_metadata(&candidate)
        .map_err(|e| format!("无法访问媒体文件 {}：{e}", candidate.display()))?;
    if link_metadata.file_type().is_symlink() {
        return Err("不允许操作符号链接。".into());
    }
    let normalized = candidate
        .canonicalize()
        .map_err(|e| format!("无法规范化媒体文件路径：{e}"))?;
    if !normalized.starts_with(&root) {
        return Err("媒体文件不在保存目录内，已阻止操作。".into());
    }
    if !normalized.is_file() || !is_supported_media_file(&normalized) {
        return Err("不是受支持的媒体文件。".into());
    }
    Ok(normalized)
}

pub fn delete_library_file(save_dir: &Path, path: &Path) -> Result<(), String> {
    let path = validate_library_file(save_dir, path)?;
    fs::remove_file(&path).map_err(|e| format!("删除媒体文件 {} 失败：{e}", path.display()))
}

fn is_supported_media_file(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if TEMPORARY_SUFFIXES
        .iter()
        .any(|suffix| file_name.ends_with(suffix))
    {
        return false;
    }
    path.extension()
        .and_then(|value| value.to_str())
        .map(|extension| SUPPORTED_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn system_time_ms(time: SystemTime) -> i64 {
    let millis = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::time::Duration;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("vidgo-library-{name}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn set_modified(path: &Path, seconds: u64) {
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.set_times(
            fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)),
        )
        .unwrap();
    }

    #[test]
    fn scan_recurses_filters_temporary_files_and_sorts_by_modified_time() {
        let root = TestDir::new("scan");
        let nested = root.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(root.path().join("newer.MP4"), b"newer").unwrap();
        fs::write(nested.join("older.mkv"), b"older").unwrap();
        fs::write(nested.join("audio.opus"), b"audio").unwrap();
        fs::write(root.path().join("notes.txt"), b"ignore").unwrap();
        fs::write(root.path().join("partial.mp4.part"), b"ignore").unwrap();
        fs::write(root.path().join("metadata.ytdl"), b"ignore").unwrap();
        fs::write(root.path().join("temporary.webm.tmp"), b"ignore").unwrap();
        set_modified(&root.path().join("newer.MP4"), 2_000);
        set_modified(&nested.join("older.mkv"), 1_000);
        set_modified(&nested.join("audio.opus"), 1_500);

        let files = scan_library(root.path()).unwrap();
        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(names, vec!["newer.MP4", "audio.opus", "older.mkv"]);
        assert_eq!(files[0].extension, "mp4");
        assert_eq!(files[0].size, 5);
        assert_eq!(files[0].modified_at, 2_000_000);
    }

    #[test]
    fn delete_rejects_files_outside_save_dir() {
        let root = TestDir::new("delete-root");
        let outside = TestDir::new("delete-outside");
        let outside_file = outside.path().join("outside.mp4");
        fs::write(&outside_file, b"keep").unwrap();

        let traversal = root.path().join("..").join(
            outside_file
                .strip_prefix(outside_file.parent().unwrap().parent().unwrap())
                .unwrap(),
        );
        let result = delete_library_file(root.path(), &outside_file);
        assert!(result.is_err());
        assert!(outside_file.exists());
        assert!(validate_library_file(root.path(), &traversal).is_err());
        assert!(outside_file.exists());
    }

    #[cfg(unix)]
    #[test]
    fn scan_and_delete_do_not_follow_symbolic_links() {
        use std::os::unix::fs::symlink;

        let root = TestDir::new("symlink-root");
        let outside = TestDir::new("symlink-outside");
        fs::write(root.path().join("real.mp4"), b"real").unwrap();
        let outside_file = outside.path().join("outside.mp4");
        fs::write(&outside_file, b"outside").unwrap();
        symlink(&outside_file, root.path().join("linked.mp4")).unwrap();
        symlink(outside.path(), root.path().join("linked-dir")).unwrap();

        let files = scan_library(root.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "real.mp4");
        assert!(delete_library_file(root.path(), &root.path().join("linked.mp4")).is_err());
        assert!(outside_file.exists());
    }
}
