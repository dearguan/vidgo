use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn binary_dir() -> PathBuf {
    // Windows: 打包后放在 resources/bin/win32-x64/ 下
    // macOS/Linux: dev 时也在 resources/bin/ 对应平台
    #[cfg(target_os = "windows")]
    let platform = "win32-x64";
    #[cfg(target_os = "macos")]
    let platform = "darwin-arm64";
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let platform = "linux-x64";

    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    // Release: next to exe -> resources/bin/<platform>/
    let release = base.join("resources").join("bin").join(platform);
    if release.exists() {
        return release;
    }
    // Dev: 项目根 bin/<platform>/
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("bin")
        .join(platform);
    if dev.exists() {
        return dev;
    }
    release
}

pub fn ytdlp_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    let name = "yt-dlp.exe";
    #[cfg(not(target_os = "windows"))]
    let name = "yt-dlp";
    binary_dir().join(name)
}

pub fn ffmpeg_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    let name = "ffmpeg.exe";
    #[cfg(not(target_os = "windows"))]
    let name = "ffmpeg";
    binary_dir().join(name)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOption {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub selector: String,
    #[serde(default)]
    pub audio_format: Option<String>,
    #[serde(default)]
    pub height: Option<u16>,
    #[serde(default)]
    pub ext: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub uploader: Option<String>,
    #[serde(default)]
    pub site: Option<String>,
    pub formats: Vec<FormatOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<MediaInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn build_download_args(
    url: &str,
    _title: &str,
    selector: &str,
    save_dir: &str,
    proxy: &str,
    cookies_from_browser: &str,
    audio_format: Option<&str>,
    height: Option<u16>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-f".into(),
        selector.into(),
        "--no-playlist".into(),
        "--no-warnings".into(),
        "--socket-timeout".into(),
        "20".into(),
        "-o".into(),
        format!("{}/%(title)s [%(height>0)jP].%(ext)s", save_dir),
        "--newline".into(),
        "--progress-template".into(),
        "download:VGP|%(status)s|%(downloaded_bytes)s|%(total_bytes)s|%(total_bytes_estimate)s|%(speed)s|%(eta)s|%(progress._percent_str)s".into(),
        "--print".into(),
        "before_dl:VGFILE|%(filepath)s".into(),
    ];
    if let Some(af) = audio_format {
        if af != "none" {
            args.push("--extract-audio".into());
            args.push("--audio-format".into());
            args.push(af.into());
            args.push("--audio-quality".into());
            args.push("0".into());
        }
    }
    if let Some(h) = height {
        args.push("--merge-output-format".into());
        args.push("mp4".into());
        // 保证 ffmpeg 合并
        let _ = h; // 已在 selector 里体现
    }
    if !proxy.is_empty() {
        args.push("--proxy".into());
        args.push(proxy.into());
    }
    if !cookies_from_browser.is_empty() {
        args.push("--cookies-from-browser".into());
        args.push(cookies_from_browser.into());
    }
    args.push(url.into());
    args
}
