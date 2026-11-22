// Simplified store — no once_cell dependency
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub type ThemeMode = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_save_dir")]
    pub save_dir: String,
    #[serde(default)]
    pub concurrency: u8,
    #[serde(default)]
    pub proxy: String,
    #[serde(default = "default_theme")]
    pub theme: ThemeMode,
    #[serde(default = "default_true")]
    pub clipboard_watch: bool,
    #[serde(default, rename = "cookieBrowser")]
    pub cookies_from_browser: String,
}

fn default_save_dir() -> String {
    if let Some(home) = dirs::home_dir() {
        home.join("Downloads")
            .join("Vidgo")
            .to_string_lossy()
            .to_string()
    } else {
        "Downloads/Vidgo".into()
    }
}
fn default_theme() -> ThemeMode {
    "system".into()
}
fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_dir: default_save_dir(),
            concurrency: 3,
            proxy: String::new(),
            theme: default_theme(),
            clipboard_watch: true,
            cookies_from_browser: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TaskStatus {
    Waiting,
    Downloading,
    Merging,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: String,
    pub url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub format_id: String,
    pub format_label: String,
    pub kind: String,
    pub selector: String,
    pub audio_format: Option<String>,
    pub height: Option<u16>,
    pub save_dir: String,
    pub status: TaskStatus,
    pub progress: f64,
    pub downloaded: u64,
    pub total: u64,
    pub speed: f64,
    pub eta: Option<f64>,
    pub error: Option<String>,
    pub error_detail: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreData {
    pub settings: Settings,
    #[serde(default)]
    pub tasks: Vec<DownloadTask>,
    #[serde(default)]
    pub subscriptions: Vec<crate::subscriptions::Subscription>,
}

pub struct AppStore {
    pub data: StoreData,
    pub path: PathBuf,
}

impl AppStore {
    fn storage_path() -> PathBuf {
        let base = dirs::data_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Vidgo").join("videogo.json")
    }

    pub fn load() -> Self {
        let path = Self::storage_path();
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<StoreData>(&s).ok())
            .unwrap_or_else(|| StoreData {
                settings: Settings::default(),
                tasks: vec![],
                subscriptions: vec![],
            });
        Self { data, path }
    }

    pub fn save_result(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
        }
        let json =
            serde_json::to_string_pretty(&self.data).map_err(|e| format!("序列化配置失败：{e}"))?;
        fs::write(&self.path, json).map_err(|e| format!("保存配置失败：{e}"))
    }

    pub fn save(&self) {
        let _ = self.save_result();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_store_without_subscriptions_deserializes_with_empty_list() {
        let data: StoreData = serde_json::from_str(
            r#"{
                "settings": {
                    "saveDir": "/tmp/vidgo",
                    "concurrency": 3,
                    "proxy": "",
                    "theme": "system",
                    "clipboardWatch": true,
                    "cookieBrowser": ""
                },
                "tasks": []
            }"#,
        )
        .unwrap();
        assert!(data.subscriptions.is_empty());
        assert!(data.tasks.is_empty());
        assert_eq!(data.settings.save_dir, "/tmp/vidgo");

        let serialized = serde_json::to_value(&data).unwrap();
        assert_eq!(
            serialized.get("subscriptions").and_then(|v| v.as_array()),
            Some(&Vec::new())
        );
    }
}
