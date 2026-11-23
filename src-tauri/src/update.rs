use crate::net;
use crate::store::AppStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const API_BASE: &str = "https://api.github.com";
const DEFAULT_REPO: &str = "dearguan/vidgo";
const MAX_NOTES_CHARS: usize = 4_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: Option<String>,
    pub asset_url: Option<String>,
    pub asset_size: Option<u64>,
    pub sha256: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: String,
    url: String,
    size: Option<u64>,
    sha256: Option<String>,
    notes: Option<String>,
    published_at: Option<String>,
}

fn parse_version(version: &str) -> [u64; 3] {
    let trimmed = version.trim().trim_start_matches(['v', 'V']);
    let mut out = [0u64; 3];
    for (index, part) in trimmed.split('.').take(3).enumerate() {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        out[index] = digits.parse().unwrap_or(0);
    }
    out
}

fn is_newer(latest: &str, current: &str) -> bool {
    let a = parse_version(latest);
    let b = parse_version(current);
    a > b
}

fn normalize_sha(digest: &str) -> Option<String> {
    digest
        .trim()
        .strip_prefix("sha256:")
        .map(|s| s.trim().to_ascii_lowercase())
}

pub async fn check(app: &AppHandle, store: &Mutex<AppStore>) -> Result<UpdateInfo, String> {
    let (proxy, token, repo) = {
        let s = store.lock().map_err(|e| e.to_string())?;
        let repo = if s.data.settings.report_repo.trim().is_empty() {
            DEFAULT_REPO.to_string()
        } else {
            s.data.settings.report_repo.trim().to_string()
        };
        (
            s.data.settings.proxy.clone(),
            s.data.settings.github_token.trim().to_string(),
            repo,
        )
    };

    let client = net::client(&proxy, Duration::from_secs(25))?;
    let current = app.package_info().version.to_string();

    // 优先读取 Release 里的 latest.json：走 CDN，不受 GitHub API 每小时 60 次的限制
    if let Some(info) = manifest_check(&client, &repo, &current).await {
        return Ok(info);
    }

    let mut request = client
        .get(format!("{API_BASE}/repos/{repo}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if !token.is_empty() {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("检查更新失败：请确认网络可访问 GitHub（{e}）"))?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            404 => "还没有可用的正式版本。".into(),
            403 => "检查更新过于频繁，请稍后再试。".into(),
            code => format!("检查更新失败（HTTP {code}）。"),
        });
    }
    let current = app.package_info().version.to_string();
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析版本信息失败：{e}"))?;
    parse_release(payload, &current)
}

async fn manifest_check(
    client: &reqwest::Client,
    repo: &str,
    current: &str,
) -> Option<UpdateInfo> {
    let url = format!("https://github.com/{repo}/releases/latest/download/latest.json");
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let payload: serde_json::Value = response.json().await.ok()?;
    parse_manifest(payload, current)
}

pub(crate) fn parse_manifest(payload: serde_json::Value, current: &str) -> Option<UpdateInfo> {
    let manifest: Manifest = serde_json::from_value(payload).ok()?;
    let latest = manifest.version.trim_start_matches(['v', 'V']).to_string();
    Some(UpdateInfo {
        available: is_newer(&latest, current),
        current: current.to_string(),
        latest,
        notes: manifest.notes,
        asset_url: Some(manifest.url),
        asset_size: manifest.size,
        sha256: manifest
            .sha256
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty()),
        published_at: manifest.published_at,
    })
}

pub(crate) fn parse_release(payload: serde_json::Value, current: &str) -> Result<UpdateInfo, String> {
    let release: Release =
        serde_json::from_value(payload).map_err(|e| format!("解析版本信息失败：{e}"))?;
    let latest = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().ends_with("x64-setup.exe"));
    let notes = release.body.map(|b| {
        if b.chars().count() > MAX_NOTES_CHARS {
            b.chars().take(MAX_NOTES_CHARS).collect()
        } else {
            b
        }
    });

    Ok(UpdateInfo {
        available: is_newer(&latest, current),
        current: current.to_string(),
        latest,
        notes,
        asset_url: asset.map(|a| a.browser_download_url.clone()),
        asset_size: asset.map(|a| a.size),
        sha256: asset.and_then(|a| a.digest.as_deref()).and_then(normalize_sha),
        published_at: release.published_at,
    })
}

fn safe_file_name(url: &str) -> String {
    let raw = url.rsplit('/').next().unwrap_or("Vidgo-setup.exe");
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if cleaned.to_ascii_lowercase().ends_with(".exe") {
        cleaned
    } else {
        "Vidgo-setup.exe".into()
    }
}

fn allowed_download_url(raw: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "更新地址无效。".to_string())?;
    if url.scheme() != "https" {
        return Err("更新地址必须使用 HTTPS。".into());
    }
    let host = url.host_str().unwrap_or_default();
    let trusted = host == "github.com"
        || host.ends_with(".github.com")
        || host.ends_with(".githubusercontent.com");
    if !trusted {
        return Err("更新地址不是受信任的来源。".into());
    }
    Ok(url)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("读取安装包失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|e| format!("校验安装包失败：{e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn update_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    base.join("Vidgo").join("update")
}

/// 更新日志路径（排查自动更新失败时给用户看）
pub fn log_path() -> PathBuf {
    update_dir().join("update.log")
}

fn append_log(line: &str) {
    let _ = std::fs::create_dir_all(update_dir());
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(file, "[{seconds}] {line}");
    }
}

fn emit_progress(app: &AppHandle, stage: &str, downloaded: u64, total: u64, speed: f64) {
    let _ = app.emit(
        "update:progress",
        serde_json::json!({
            "stage": stage,
            "downloaded": downloaded,
            "total": total,
            "speed": speed,
        }),
    );
}

#[cfg(target_os = "windows")]
fn launch_installer(setup: &Path) -> Result<(), String> {
    let app_exe = std::env::current_exe().map_err(|e| format!("定位当前程序失败：{e}"))?;
    let script_path = update_dir().join("run-update.cmd");
    let log = log_path();
    // 用脚本文件而不是一行 cmd：便于记录安装器退出码，失败后也能复查
    let script = format!(
        "@echo off\r\n\
         ping -n 4 127.0.0.1 >nul\r\n\
         \"{}\" /S /currentuser\r\n\
         set RC=%ERRORLEVEL%\r\n\
         >>\"{}\" echo installer exit=%RC%\r\n\
         start \"\" \"{}\"\r\n",
        setup.display(),
        log.display(),
        app_exe.display()
    );
    std::fs::write(&script_path, script).map_err(|e| format!("写入更新脚本失败：{e}"))?;
    append_log(&format!(
        "launch installer: {} -> {}",
        setup.display(),
        app_exe.display()
    ));

    let mut command = std::process::Command::new("cmd");
    crate::ytdlp::hide_console(&mut command);
    command
        .args(["/C", &script_path.to_string_lossy()])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动安装程序失败：{e}"))
}

pub async fn install(
    app: &AppHandle,
    store: &Mutex<AppStore>,
    asset_url: &str,
    sha256: Option<String>,
    expected_size: Option<u64>,
) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("自动安装仅支持 Windows 安装包；当前系统可在开发环境自行测试。".into());
    }
    let url = allowed_download_url(asset_url)?;
    let proxy = {
        let s = store.lock().map_err(|e| e.to_string())?;
        s.data.settings.proxy.clone()
    };

    append_log(&format!("download start: {url}"));
    emit_progress(app, "downloading", 0, expected_size.unwrap_or(0), 0.0);

    let client = net::download_client(&proxy)?;
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|e| {
            append_log(&format!("download request failed: {e}"));
            format!("下载更新失败：{e}")
        })?;
    if !response.status().is_success() {
        let code = response.status().as_u16();
        append_log(&format!("download http error: {code}"));
        return Err(format!("下载更新失败（HTTP {code}）。"));
    }

    let total = response.content_length().or(expected_size).unwrap_or(0);
    let dir = update_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建更新目录失败：{e}"))?;
    let target: PathBuf = dir.join(safe_file_name(asset_url));
    let _ = std::fs::remove_file(&target);
    let mut file = std::fs::File::create(&target).map_err(|e| format!("写入安装包失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    let mut speed_window_start = std::time::Instant::now();
    let mut speed_window_bytes: u64 = 0;
    let mut speed: f64 = 0.0;

    while let Some(chunk) = response.chunk().await.map_err(|e| {
        append_log(&format!("download interrupted at {downloaded}: {e}"));
        format!("下载更新中断：{e}")
    })? {
        file.write_all(&chunk)
            .map_err(|e| format!("写入安装包失败：{e}"))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        speed_window_bytes += chunk.len() as u64;
        let elapsed = speed_window_start.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            speed = speed_window_bytes as f64 / elapsed;
            speed_window_start = std::time::Instant::now();
            speed_window_bytes = 0;
        }
        if last_emit.elapsed() >= Duration::from_millis(300) {
            last_emit = std::time::Instant::now();
            emit_progress(app, "downloading", downloaded, total, speed);
        }
    }
    file.flush().map_err(|e| format!("写入安装包失败：{e}"))?;
    drop(file);
    append_log(&format!("download done: {downloaded} / {total} bytes"));

    emit_progress(app, "verifying", downloaded, total, 0.0);
    if let Some(expected) = expected_size {
        if expected > 0 && downloaded != expected {
            append_log(&format!("size mismatch: expected {expected}, got {downloaded}"));
            let _ = std::fs::remove_file(&target);
            return Err(format!(
                "安装包大小不一致（期望 {expected} 字节，实际 {downloaded} 字节），已取消安装。"
            ));
        }
    }
    if let Some(expected) = sha256 {
        let actual = sha256_file(&target)?;
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            append_log("sha256 mismatch");
            let _ = std::fs::remove_file(&target);
            return Err("安装包校验失败，可能下载不完整，已取消安装。".into());
        }
        append_log("sha256 verified");
    }

    emit_progress(app, "installing", downloaded, total, 0.0);

    #[cfg(target_os = "windows")]
    {
        launch_installer(&target)?;
        app.exit(0);
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::fs::remove_file(&target);
        Err("当前系统不支持自动安装。".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.4.4", "0.4.3"));
        assert!(is_newer("v0.5.0", "0.4.99"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.4.3", "0.4.3"));
        assert!(!is_newer("0.4.2", "0.4.3"));
    }

    #[test]
    fn only_trusts_github_download_hosts() {
        assert!(allowed_download_url(
            "https://github.com/dearguan/vidgo/releases/download/v0.4.4/Vidgo_0.4.4_x64-setup.exe"
        )
        .is_ok());
        assert!(allowed_download_url("http://github.com/x/y.exe").is_err());
        assert!(allowed_download_url("https://evil.example.com/y.exe").is_err());
    }

    #[test]
    fn parses_release_payload() {
        let payload = serde_json::json!({
            "tag_name": "v0.4.4",
            "body": "修复解析问题",
            "published_at": "2026-11-22T00:00:00Z",
            "assets": [
                {
                    "name": "Vidgo_0.4.4_x64-setup.exe",
                    "browser_download_url": "https://github.com/dearguan/vidgo/releases/download/v0.4.4/Vidgo_0.4.4_x64-setup.exe",
                    "size": 57144606u64,
                    "digest": "sha256:ABCDEF"
                },
                {
                    "name": "notes.zip",
                    "browser_download_url": "https://example.com/notes.zip",
                    "size": 12u64
                }
            ]
        });
        let info = parse_release(payload, "0.4.3").unwrap();
        assert!(info.available);
        assert_eq!(info.latest, "0.4.4");
        assert_eq!(info.asset_size, Some(57144606));
        assert_eq!(info.sha256.as_deref(), Some("abcdef"));
        assert_eq!(info.notes.as_deref(), Some("修复解析问题"));
        assert!(info
            .asset_url
            .as_deref()
            .unwrap()
            .ends_with("Vidgo_0.4.4_x64-setup.exe"));
    }

    #[test]
    fn parses_latest_manifest() {
        let payload = serde_json::json!({
            "version": "0.4.4",
            "url": "https://github.com/dearguan/vidgo/releases/download/v0.4.4/Vidgo_0.4.4_x64-setup.exe",
            "size": 57144606u64,
            "sha256": "ABC123",
            "notes": "修复解析",
            "publishedAt": "2026-11-22T00:00:00Z"
        });
        let info = parse_manifest(payload, "0.4.3").unwrap();
        assert!(info.available);
        assert_eq!(info.latest, "0.4.4");
        assert_eq!(info.sha256.as_deref(), Some("abc123"));
        assert_eq!(info.asset_size, Some(57144606));
        assert!(parse_manifest(serde_json::json!({ "version": "1.0.0" }), "0.4.3").is_none());
    }

    #[test]
    fn sanitizes_installer_name() {
        assert_eq!(
            safe_file_name("https://github.com/a/b/releases/download/v1/Vidgo_1.0_x64-setup.exe"),
            "Vidgo_1.0_x64-setup.exe"
        );
        assert_eq!(safe_file_name("https://github.com/a/b/../../evil"), "Vidgo-setup.exe");
    }
}
