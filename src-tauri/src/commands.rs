use crate::browser::{SnifferPayload, SnifferState};
use crate::store::{AppStore, DownloadTask, Settings, TaskStatus};
use crate::ytdlp::{self, FormatOption, MediaInfo, ParseResult, ytdlp_path};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use std::process::Stdio;
use uuid::Uuid;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub ytdlp: String,
}

fn current_save_dir(state: &State<'_, Mutex<AppStore>>) -> Result<PathBuf, String> {
    let s = state.inner().lock().map_err(|e| e.to_string())?;
    Ok(PathBuf::from(&s.data.settings.save_dir))
}

/// 修改任务并持久化 + 通知前端。避免在多处重复加锁样板。
fn with_task<F>(state: &State<'_, Mutex<AppStore>>, app: &AppHandle, id: &str, f: F)
where
    F: FnOnce(&mut DownloadTask),
{
    let mut s = match state.inner().lock() {
        Ok(s) => s,
        Err(_) => return,
    };
    if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id) {
        f(t);
    }
    s.save();
    let _ = app.emit("tasks:changed", s.data.tasks.clone());
}

const MEDIA_EXTS: [&str; 8] = ["mp4", "m4a", "mp3", "webm", "mkv", "mov", "flv", "aac"];

/// 校验文件头，识别「HTML/JSON 错误页被存成 .mp4」这类假成功。
fn looks_like_media(path: &Path) -> bool {
    use std::io::Read;
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut head = [0u8; 16];
    let n = match file.read(&mut head) {
        Ok(n) => n,
        Err(_) => return false,
    };
    if n < 4 {
        return false;
    }
    let h = &head[..n];
    // mp4 / m4a / mov：4 字节 box 长度 + "ftyp"
    if n >= 8 && &h[4..8] == b"ftyp" {
        return true;
    }
    // webm / mkv
    if h.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return true;
    }
    // mp3：ID3 标签或 MPEG 帧同步
    if h.starts_with(b"ID3") || (h[0] == 0xFF && (h[1] & 0xE0) == 0xE0) {
        return true;
    }
    // flv
    h.starts_with(b"FLV")
}

/// 找到下载目录里本次任务开始后写入的最新媒体文件（兜底 yt-dlp 没打印路径的情况）。
fn newest_media_since(dir: &Path, since: SystemTime) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !MEDIA_EXTS.contains(&ext.as_str()) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() == 0 {
            continue;
        }
        let modified = match meta.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };
        // 允许 2 秒时钟误差
        if modified + Duration::from_secs(2) < since {
            continue;
        }
        if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, p)| p)
}

fn tail_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    text.chars().skip(count - max).collect()
}


fn open_with_system(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开 {} 失败：{e}", path.display()))
}

fn get_ytdlp_ver() -> String {
    let mut command = std::process::Command::new(ytdlp_path());
    ytdlp::hide_console(&mut command);
    command
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// 供问题上报模块附带解析引擎版本
pub(crate) fn ytdlp_version() -> String {
    get_ytdlp_ver()
}

#[cfg(target_os = "windows")]
fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开链接失败：{e}"))
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开链接失败：{e}"))
}

#[cfg(target_os = "linux")]
fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开链接失败：{e}"))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn store_tasks(
    app: &AppHandle,
    state: &State<'_, Mutex<AppStore>>,
    f: impl FnOnce(&mut AppStore),
) -> Result<(), String> {
    let mut s = state.inner().lock().map_err(|e| e.to_string())?;
    f(&mut s);
    s.save();
    let _ = app.emit("tasks:changed", s.data.tasks.clone());
    Ok(())
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle) -> Result<AppInfo, String> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        ytdlp: get_ytdlp_ver(),
    })
}

#[tauri::command]
pub async fn get_tasks(state: State<'_, Mutex<AppStore>>) -> Result<Vec<DownloadTask>, String> {
    let s = state.inner().lock().map_err(|e| e.to_string())?;
    let mut tasks = s.data.tasks.clone();
    // 旧记录可能没有记录大小，用磁盘文件真实大小补齐展示
    for task in tasks.iter_mut() {
        if matches!(task.status, TaskStatus::Completed) && task.total == 0 {
            if let Some(path) = task.file_path.as_deref() {
                if let Ok(meta) = fs::metadata(path) {
                    if meta.is_file() && meta.len() > 0 {
                        task.total = meta.len();
                        task.downloaded = meta.len();
                    }
                }
            }
        }
    }
    Ok(tasks)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Mutex<AppStore>>) -> Result<Settings, String> {
    let s = state.inner().lock().map_err(|e| e.to_string())?;
    Ok(s.data.settings.clone())
}

#[tauri::command]
pub async fn set_settings(
    state: State<'_, Mutex<AppStore>>,
    new_settings: serde_json::Value,
) -> Result<Settings, String> {
    let mut s = state.inner().lock().map_err(|e| e.to_string())?;
    let mut current = serde_json::to_value(&s.data.settings)
        .map_err(|e| format!("读取当前设置失败：{e}"))?;
    if let (Some(target), Some(patch)) = (current.as_object_mut(), new_settings.as_object()) {
        for (key, value) in patch {
            target.insert(key.clone(), value.clone());
        }
    }
    let merged: Settings =
        serde_json::from_value(current).map_err(|e| format!("设置格式不正确：{e}"))?;
    s.data.settings = merged.clone();
    s.save_result()?;
    Ok(merged)
}

#[tauri::command]
pub async fn parse_media(state: State<'_, Mutex<AppStore>>, url: String) -> Result<ParseResult, String> {
    let settings = {
        let s = state.inner().lock().unwrap();
        s.data.settings.clone()
    };

    let Some(url) = ytdlp::sanitize_url(&url) else {
        return Ok(ParseResult {
            ok: false,
            info: None,
            message: Some("链接无效：请粘贴完整的 http/https 地址。".into()),
            detail: None,
            needs_login: false,
            browser_cookies: false,
        });
    };
    let url = ytdlp::canonicalize_url(&url);

    if ytdlp::is_douyin_url(&url) {
        return Ok(
            match ytdlp::resolve_douyin(&url, &ytdlp::effective_proxy(&settings, &url)).await {
                Ok(info) => ParseResult {
                    ok: true,
                    info: Some(info),
                    message: None,
                    detail: None,
                    needs_login: false,
                    browser_cookies: false,
                },
                Err(e) => ParseResult {
                    ok: false,
                    info: None,
                    message: Some(
                        "抖音解析失败。请先在「设置 → 站点登录」登录抖音，再重试。".into(),
                    ),
                    detail: Some(e),
                    needs_login: true,
                    browser_cookies: false,
                },
            },
        );
    }

    if ytdlp::is_hongguo_url(&url) {
        return Ok(match ytdlp::resolve_hongguo(&url, &ytdlp::effective_proxy(&settings, &url)).await {
            Ok(media) => ParseResult {
                ok: true,
                info: Some(media.info),
                message: None,
                detail: None,
                needs_login: false,
                browser_cookies: false,
            },
            Err(e) => ParseResult {
                ok: false,
                info: None,
                message: Some("红果视频解析失败，请确认链接有效或稍后重试。".into()),
                detail: Some(e),
                needs_login: false,
                browser_cookies: false,
            },
        });
    }

    let mut base_args: Vec<String> = vec![
        "-J".into(), "--no-playlist".into(), "--no-warnings".into(),
        "--socket-timeout".into(), "20".into(),
    ];
    let proxy = ytdlp::effective_proxy(&settings, &url);
    if !proxy.is_empty() {
        base_args.push("--proxy".into());
        base_args.push(proxy);
    }

    // 先只用站点登录 Cookie；若站点 Cookie 不足以通过校验，再合并系统浏览器 Cookie 重试一次
    let mut attempt = 0usize;
    let (needs_login, mut message, detail) = loop {
        let cookies = if attempt == 0 {
            ytdlp::cookie_args(&settings, &url)
        } else if ytdlp::has_site_cookie_file(&url) {
            ytdlp::cookie_args_merged(&settings, &url)
        } else {
            ytdlp::cookie_args_browser(&settings)
        };
        let mut args = base_args.clone();
        args.extend(cookies.args.iter().cloned());
        args.push(url.clone());

        let mut command = std::process::Command::new(ytdlp_path());
        ytdlp::hide_console(&mut command);
        let output = match command.args(&args).output() {
            Ok(o) => o,
            Err(e) => {
                return Ok(ParseResult {
                    ok: false,
                    info: None,
                    message: Some(format!("无法启动解析引擎：{}", e)),
                    detail: Some(e.to_string()),
                    needs_login: false,
                    browser_cookies: false,
                });
            }
        };

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            return Ok(match parse_json(&stdout, &url) {
                Ok(info) => ParseResult {
                    ok: true,
                    info: Some(info),
                    message: None,
                    detail: None,
                    needs_login: false,
                    browser_cookies: attempt > 0,
                },
                Err(e) => ParseResult {
                    ok: false, info: None,
                    message: Some("解析结果异常，请稍后重试。".into()),
                    detail: Some(e.to_string()),
                    needs_login: false,
                    browser_cookies: false,
                },
            });
        }

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let cookie_related = needs_login_error(&stderr);
        let can_merge = attempt == 0
            && cookie_related
            && ytdlp::has_browser_cookie_source(&settings);
        if !can_merge {
            break (cookie_related, humanize_err(&stderr), stderr);
        }
        attempt = 1;
    };

    if let Some(login) = ytdlp::site_login_for(&url) {
        if needs_login {
            message = format!(
                "{message} 已尝试 {} 的 {} 个 Cookie{}，仍被站点拒绝，请在设置中「重新登录」该站点。",
                login.domain,
                login.cookie_count,
                if attempt > 0 { "与系统浏览器 Cookie" } else { "" }
            );
        }
    }
    Ok(ParseResult {
        ok: false,
        info: None,
        message: Some(message),
        needs_login,
        detail: Some(detail),
        browser_cookies: false,
    })
}

/// 判断失败原因是否属于「需要登录 / Cookie」，用于引导用户去内置浏览器登录
fn needs_login_error(detail: &str) -> bool {
    const KEYS: [&str; 10] = [
        "Fresh cookies",
        "Sign in to confirm",
        "login required",
        "Private video",
        "HTTP Error 403",
        "Forbidden",
        "Could not copy Chrome cookie database",
        "cookie database",
        "could not decrypt",
        "failed to decrypt",
    ];
    let lower = detail.to_lowercase();
    KEYS.iter().any(|key| lower.contains(&key.to_lowercase()))
}

fn humanize_err(detail: &str) -> String {
    let rules: Vec<(&str, &str)> = vec![
        ("Unsupported URL|no suitable info extractor", "当前网站暂时无法解析。可能是网站更新导致解析器失效。"),
        ("Sign in to confirm|login required|Private video", "该视频需要登录才能访问，请在「设置 → 站点登录」用内置浏览器登录后重试。"),
        ("HTTP Error 403|Forbidden", "网站拒绝访问（403）。可能是地区限制或需要登录。"),
        ("HTTP Error 404|not found", "视频不存在或已被删除（404）。"),
        ("getaddrinfo|timed out|Connection refused|SSL", "网络连接失败，请检查网络或代理设置。"),
        ("Could not copy Chrome cookie database|failed to decrypt|could not decrypt|cookie database", "无法读取系统浏览器的 Cookie（浏览器正在运行或新版加密保护）。请完全退出该浏览器后重试，或在「设置 → 站点登录」用内置浏览器登录该网站。"),
        ("Fresh cookies", "站点拒绝了当前 Cookie：请先确认已保存该站点登录，仍失败则可能是网站启用了签名校验（如抖音），改用「用内置浏览器捕获」在页面播放后添加媒体。"),
        ("ffmpeg|ffprobe", "FFmpeg 处理失败，请检查组件是否完整。"),
        ("No space left|ENOSPC|disk quota", "磁盘空间不足，请更换保存位置。"),
        ("Permission denied|EACCES|EPERM", "没有写入权限，请更换保存位置。"),
    ];
    for (re, msg) in rules {
        if regex::Regex::new(re).map(|r| r.is_match(detail)).unwrap_or(false) {
            return msg.into();
        }
    }
    "任务失败，展开详情可查看原始错误信息。".into()
}

fn parse_json(raw: &str, source_url: &str) -> Result<MediaInfo, String> {
    #[derive(serde::Deserialize)]
    struct F { height: Option<u16>, vcodec: Option<String> }
    #[derive(serde::Deserialize)]
    struct R {
        id: Option<String>, title: Option<String>, thumbnail: Option<String>,
        duration: Option<f64>, uploader: Option<String>,
        webpage_url: Option<String>, original_url: Option<String>,
        extractor_key: Option<String>, extractor: Option<String>,
        formats: Option<Vec<F>>,
    }
    let r: R = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let formats = r.formats.unwrap_or_default();
    let mut hs: Vec<u16> = formats
        .iter().filter(|f| f.vcodec.as_deref().is_some_and(|v| v != "none"))
        .filter_map(|f| f.height).collect();
    hs.sort_unstable_by(|a, b| b.cmp(a));
    hs.dedup();

    let mut out: Vec<FormatOption> = vec![FormatOption {
        id: "best".into(), label: "最佳画质（自动）".into(),
        kind: "video".into(), selector: "bv*+ba/b".into(),
        audio_format: None, height: None, ext: Some("mp4".into()),
    }];
    for h in &hs {
        out.push(FormatOption {
            id: format!("h{}v", h), label: format!("{}P", h),
            kind: "video".into(),
            selector: format!("bv*[height<={}]+ba/b[height<={}]", h, h),
            audio_format: None, height: Some(*h), ext: Some("mp4".into()),
        });
    }
    for (lid, llabel, lsel, laf) in [
        ("audio-mp3", "仅下载音频 MP3", "ba", Some("mp3")),
        ("audio-m4a", "仅下载音频 M4A", "ba", Some("m4a")),
    ] {
        out.push(FormatOption {
            id: lid.into(), label: llabel.into(), kind: "audio".into(),
            selector: lsel.into(), audio_format: laf.map(|s| s.into()),
            height: None, ext: Some((laf.unwrap_or("mp4")).into()),
        });
    }

    Ok(MediaInfo {
        id: r.id.unwrap_or_default(),
        url: r
            .webpage_url
            .or(r.original_url)
            .unwrap_or_else(|| source_url.to_string()),
        title: r.title.unwrap_or_else(|| "视频".into()),
        thumbnail: r.thumbnail, duration: r.duration, uploader: r.uploader,
        site: r.extractor_key.or(r.extractor), formats: out,
    })
}

#[tauri::command]
pub async fn parse_cancel() -> Result<(), String> { Ok(()) }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDownloadPayload {
    pub url: String, pub title: String, pub thumbnail: Option<String>,
    pub duration: Option<f64>, pub save_dir: String, pub option: FormatOption,
    /// 解析时合并了系统浏览器 Cookie，下载时沿用
    #[serde(default)]
    pub browser_cookies: bool,
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle, state: State<'_, Mutex<AppStore>>, payload: StartDownloadPayload,
) -> Result<DownloadTask, String> {
    let id = Uuid::new_v4().to_string();
    let task = DownloadTask {
        id: id.clone(), url: payload.url.clone(), title: payload.title.clone(),
        thumbnail: payload.thumbnail, duration: payload.duration,
        format_id: payload.option.id.clone(), format_label: payload.option.label.clone(),
        kind: payload.option.kind.clone(), selector: payload.option.selector.clone(),
        audio_format: payload.option.audio_format.clone(), height: payload.option.height,
        save_dir: payload.save_dir.clone(),
        status: TaskStatus::Waiting, progress: 0.0, downloaded: 0, total: 0,
        speed: 0.0, eta: None, error: None, error_detail: None,
        needs_login: false,
        browser_cookies: payload.browser_cookies,
        file_path: None, completed_at: None, created_at: now_ms(),
    };

    store_tasks(&app, &state, |s| {
        s.data.tasks.insert(0, task.clone());
        if s.data.tasks.len() > 200 { s.data.tasks.truncate(200); }
    })?;

    let settings = {
        let s = state.inner().lock().unwrap();
        s.data.settings.clone()
    };
    let app2 = app.clone();
    let id2 = id.clone();
    let task_clone = task.clone();
    tokio::spawn(async move {
        let state = match app2.try_state::<Mutex<AppStore>>() {
            Some(s) => s,
            None => return,
        };

        if let Err(e) = fs::create_dir_all(&task_clone.save_dir) {
            let mut s = state.inner().lock().unwrap();
            if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id2) {
                t.status = TaskStatus::Failed;
                t.error = Some(format!("保存目录不可用：{}", e));
            }
            s.save();
            let _ = app2.emit("tasks:changed", s.data.tasks.clone());
            return;
        }

        let is_hongguo = ytdlp::is_hongguo_url(&task_clone.url);
        let mut download_url = ytdlp::canonicalize_url(&task_clone.url);
        if is_hongguo {
            match ytdlp::resolve_hongguo(&download_url, &ytdlp::effective_proxy(&settings, &download_url)).await {
                Ok(media) => download_url = media.play_url,
                Err(e) => {
                    let mut s = state.inner().lock().unwrap();
                    if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id2) {
                        t.status = TaskStatus::Failed;
                        t.error = Some("红果视频地址获取失败，请重新解析后再下载。".into());
                        t.error_detail = Some(e);
                    }
                    s.save();
                    let _ = app2.emit("tasks:changed", s.data.tasks.clone());
                    return;
                }
            }
        } else if ytdlp::is_douyin_url(&download_url) {
            match ytdlp::resolve_douyin(&download_url, &ytdlp::effective_proxy(&settings, &download_url)).await {
                Ok(info) => download_url = info.url,
                Err(e) => {
                    let mut s = state.inner().lock().unwrap();
                    if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id2) {
                        t.status = TaskStatus::Failed;
                        t.error = Some("抖音视频地址获取失败，请重新解析后再下载。".into());
                        t.error_detail = Some(e);
                    }
                    s.save();
                    let _ = app2.emit("tasks:changed", s.data.tasks.clone());
                    return;
                }
            }
        }
        // Cookie 来源兜底：浏览器 Cookie 读不到时去掉；站点 Cookie 失效时再加上浏览器
        let mut modes: Vec<bool> = vec![task_clone.browser_cookies];
        if ytdlp::has_browser_cookie_source(&settings) {
            modes.push(!task_clone.browser_cookies);
        }
        for (attempt, &use_browser) in modes.iter().enumerate() {
            let cookies = if use_browser && ytdlp::has_site_cookie_file(&download_url) {
                ytdlp::cookie_args_merged(&settings, &download_url)
            } else if use_browser {
                ytdlp::cookie_args_browser(&settings)
            } else {
                ytdlp::cookie_args(&settings, &download_url)
            };
        let mut args = ytdlp::build_download_args(
            &download_url, &task_clone.selector, &task_clone.save_dir,
            &ytdlp::effective_proxy(&settings, &download_url), &cookies.args,
            task_clone.audio_format.as_deref(), task_clone.height,
        );
        if is_hongguo {
            let mut with_headers = vec![
                "--user-agent".into(),
                ytdlp::HONGGUO_USER_AGENT.into(),
                "--add-header".into(),
                format!("Referer:{}", ytdlp::HONGGUO_REFERER),
            ];
            with_headers.extend(args);
            args = with_headers;
        }
        let mut cmd = Command::new(ytdlp_path());
        ytdlp::hide_console_async(&mut cmd);
        let mut child = match cmd.args(&args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(c) => c,
            Err(e) => {
                let mut s = state.inner().lock().unwrap();
                if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id2) {
                    t.status = TaskStatus::Failed;
                    t.error = Some(format!("无法启动 yt-dlp：{}", e));
                }
                s.save();
                let _ = app2.emit("tasks:changed", s.data.tasks.clone());
                return;
            }
        };
        with_task(&state, &app2, &id2, |t| {
            t.status = TaskStatus::Downloading;
        });

        let started = SystemTime::now();
        let mut reported: Option<PathBuf> = None;
        let mut final_reported: Option<PathBuf> = None;
        let mut stderr_buf = String::new();
        let mut base_bytes: u64 = 0;
        let mut prev_status = String::new();
        let mut last_emit = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut out_lines = BufReader::new(stdout.unwrap()).lines();
        let mut err_lines = BufReader::new(stderr.unwrap()).lines();
        let mut out_done = false;
        let mut err_done = false;
        while !(out_done && err_done) {
            tokio::select! {
                line = out_lines.next_line(), if !out_done => {
                    match line {
                        Ok(Some(line)) => {
                            if let Some(rest) = line.strip_prefix("VGFILE|") {
                                reported = Some(PathBuf::from(rest.trim()));
                            } else if let Some(rest) = line.strip_prefix("VGFEND|") {
                                final_reported = Some(PathBuf::from(rest.trim()));
                            } else if let Some(rest) = line.strip_prefix("VGP|") {
                                let parts: Vec<&str> = rest.split('|').collect();
                                let stream_status = parts.first().copied().unwrap_or("");
                                let num = |i: usize| -> u64 {
                                    parts
                                        .get(i)
                                        .and_then(|v| v.trim().parse::<f64>().ok())
                                        .filter(|n| *n > 0.0)
                                        .unwrap_or(0.0) as u64
                                };
                                let down = num(1);
                                let total = match num(2) {
                                    0 => num(3),
                                    t => t,
                                };
                                let speed = parts
                                    .get(4)
                                    .and_then(|v| v.trim().parse::<f64>().ok())
                                    .filter(|n| *n > 0.0)
                                    .unwrap_or(0.0);
                                let eta = parts
                                    .get(5)
                                    .and_then(|v| v.trim().parse::<f64>().ok())
                                    .filter(|n| *n > 0.0);
                                let pct = parts
                                    .get(6)
                                    .and_then(|v| v.trim().trim_end_matches('%').trim().parse::<f64>().ok())
                                    .unwrap_or(0.0);
                                if stream_status == "finished" && prev_status != "finished" {
                                    base_bytes += down.max(total);
                                }
                                prev_status = stream_status.to_string();
                                let downloaded = base_bytes + down;
                                let total_bytes = base_bytes + total;
                                let progress = if total_bytes > 0 {
                                    downloaded as f64 / total_bytes as f64 * 100.0
                                } else {
                                    pct
                                };
                                if last_emit.elapsed() >= Duration::from_millis(400) {
                                    last_emit = Instant::now();
                                    with_task(&state, &app2, &id2, |t| {
                                        t.downloaded = downloaded;
                                        t.total = total_bytes;
                                        t.speed = speed;
                                        t.eta = eta;
                                        if stream_status != "finished" && progress > t.progress {
                                            t.progress = progress.min(99.5);
                                        }
                                    });
                                }
                            } else if line.contains("[Merger]")
                                || line.contains("Merging formats")
                                || line.contains("[ExtractAudio]")
                            {
                                with_task(&state, &app2, &id2, |t| {
                                    t.status = TaskStatus::Merging;
                                    t.progress = 99.5;
                                    t.speed = 0.0;
                                    t.eta = None;
                                });
                            }
                        }
                        Ok(None) => out_done = true,
                        Err(_) => out_done = true,
                    }
                }
                line = err_lines.next_line(), if !err_done => {
                    match line {
                        Ok(Some(line)) => {
                            stderr_buf.push_str(&line);
                            stderr_buf.push('\n');
                            if stderr_buf.len() > 20_000 {
                                stderr_buf = tail_chars(&stderr_buf, 20_000);
                            }
                        }
                        _ => err_done = true,
                    }
                }
            }
        }

        let wait = child.wait().await;

        // 用户已取消 / 暂停的任务不要覆盖状态
        let stopped = {
            let s = state.inner().lock().unwrap();
            s.data
                .tasks
                .iter()
                .find(|t| t.id == id2)
                .is_some_and(|t| matches!(&t.status, TaskStatus::Cancelled | TaskStatus::Paused))
        };
        if stopped {
            return;
        }

        match wait {
            Ok(status) if status.success() => {
                let found = final_reported
                    .filter(|p| p.is_file())
                    .or_else(|| reported.filter(|p| p.is_file()))
                    .or_else(|| newest_media_since(Path::new(&task_clone.save_dir), started));
                match found {
                    Some(path) if looks_like_media(&path) => {
                        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        let file_path = path.to_string_lossy().to_string();
                        with_task(&state, &app2, &id2, |t| {
                            t.status = TaskStatus::Completed;
                            t.progress = 100.0;
                            t.completed_at = Some(now_ms());
                            t.speed = 0.0;
                            t.eta = None;
                            if size > 0 {
                                t.total = size;
                                t.downloaded = size;
                            }
                            t.file_path = Some(file_path);
                        });
                    }
                    Some(path) => {
                        let detail = format!(
                            "生成的文件无法识别为视频/音频（可能是网页或错误内容）：{}\n\n--- yt-dlp 输出 ---\n{}",
                            path.display(),
                            tail_chars(&stderr_buf, 4_000)
                        );
                        with_task(&state, &app2, &id2, |t| {
                            t.status = TaskStatus::Failed;
                            t.speed = 0.0;
                            t.eta = None;
                            t.error = Some("下载到的文件无法播放（可能被网站拦截或 Cookie 失效），请重试或配置 Cookie。".into());
                            t.error_detail = Some(detail);
                            t.needs_login = true;
                        });
                    }
                    None => {
                        with_task(&state, &app2, &id2, |t| {
                            t.status = TaskStatus::Failed;
                            t.speed = 0.0;
                            t.eta = None;
                            t.error = Some("下载已结束但没有生成文件（可能被网站拦截或 Cookie 失效）。".into());
                            t.error_detail = Some(tail_chars(&stderr_buf, 4_000));
                            t.needs_login = true;
                        });
                    }
                }
            }
            Ok(status) => {
                // 换一种 Cookie 来源再试一次：浏览器库被占用就去掉浏览器，站点 Cookie 失效就加上浏览器
                let can_retry = attempt + 1 < modes.len()
                    && if use_browser {
                        ytdlp::is_browser_cookie_error(&stderr_buf)
                    } else {
                        needs_login_error(&stderr_buf)
                    };
                if can_retry {
                    with_task(&state, &app2, &id2, |t| {
                        t.status = TaskStatus::Downloading;
                        t.progress = 0.0;
                        t.downloaded = 0;
                        t.total = 0;
                        t.speed = 0.0;
                        t.eta = None;
                    });
                    continue;
                }
                let detail = format!(
                    "{}\n\n--- 退出状态 {} ---\n--- yt-dlp 输出 ---\n{}",
                    task_clone.url,
                    status,
                    tail_chars(&stderr_buf, 4_000)
                );
                with_task(&state, &app2, &id2, |t| {
                    t.status = TaskStatus::Failed;
                    t.speed = 0.0;
                    t.eta = None;
                    t.error = Some(humanize_err(&stderr_buf));
                    t.error_detail = Some(detail);
                    t.needs_login = needs_login_error(&stderr_buf);
                });
            }
            Err(e) => {
                with_task(&state, &app2, &id2, |t| {
                    t.status = TaskStatus::Failed;
                    t.speed = 0.0;
                    t.eta = None;
                    t.error = Some(e.to_string());
                });
            }
        }
        }
    });

    Ok(task)
}

#[tauri::command]
pub async fn pause_task(app: AppHandle, state: State<'_, Mutex<AppStore>>, id: String) -> Result<(), String> {
    store_tasks(&app, &state, |s| {
        if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id) {
            t.status = TaskStatus::Paused; t.speed = 0.0; t.eta = None;
        }
    })
}

#[tauri::command]
pub async fn resume_task(app: AppHandle, state: State<'_, Mutex<AppStore>>, id: String) -> Result<(), String> {
    store_tasks(&app, &state, |s| {
        if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id) {
            if matches!(t.status, TaskStatus::Paused | TaskStatus::Failed | TaskStatus::Cancelled) {
                t.status = TaskStatus::Waiting; t.error = None; t.error_detail = None;
            }
        }
    })
}

#[tauri::command]
pub async fn cancel_task(app: AppHandle, state: State<'_, Mutex<AppStore>>, id: String) -> Result<(), String> {
    store_tasks(&app, &state, |s| {
        if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id) {
            t.status = TaskStatus::Cancelled; t.speed = 0.0; t.eta = None;
        }
    })
}

#[tauri::command]
pub async fn remove_task(
    app: AppHandle, state: State<'_, Mutex<AppStore>>, id: String,
    delete_file: Option<bool>,
) -> Result<(), String> {
    let delete_file = delete_file.unwrap_or(false);
    let file_to_del = {
        let s = state.inner().lock().map_err(|e| e.to_string())?;
        if delete_file {
            s.data.tasks.iter().find(|t| t.id == id).and_then(|t| t.file_path.clone())
        } else { None }
    };
    store_tasks(&app, &state, |s| {
        s.data.tasks.retain(|t| t.id != id);
    })?;
    if let Some(fp) = file_to_del { let _ = fs::remove_file(&fp); }
    Ok(())
}

#[tauri::command]
pub async fn redownload_task(
    app: AppHandle, state: State<'_, Mutex<AppStore>>, id: String,
) -> Result<Option<DownloadTask>, String> {
    let orig = {
        let s = state.inner().lock().map_err(|e| e.to_string())?;
        s.data.tasks.iter().find(|t| t.id == id).cloned()
    };
    match orig {
        Some(o) => {
            let payload = StartDownloadPayload {
                url: o.url.clone(), title: o.title.clone(), thumbnail: o.thumbnail.clone(),
                duration: o.duration, save_dir: o.save_dir.clone(),
                browser_cookies: o.browser_cookies,
                option: FormatOption {
                    id: o.format_id.clone(), label: o.format_label.clone(),
                    kind: o.kind.clone(), selector: o.selector.clone(),
                    audio_format: o.audio_format.clone(), height: o.height, ext: None,
                },
            };
            Ok(Some(start_download(app, state, payload).await?))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn open_save_dir(state: State<'_, Mutex<AppStore>>) -> Result<(), String> {
    let dir = {
        let s = state.inner().lock().map_err(|e| e.to_string())?;
        s.data.settings.save_dir.clone()
    };
    let _ = fs::create_dir_all(&dir);
    open_with_system(Path::new(&dir))
}

fn task_file_path(state: &State<'_, Mutex<AppStore>>, id: &str) -> Result<PathBuf, String> {
    let raw = {
        let s = state.inner().lock().map_err(|e| e.to_string())?;
        s.data
            .tasks
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.file_path.clone())
    };
    let raw = raw.ok_or_else(|| "该任务没有可打开的文件记录，请重新下载。".to_string())?;
    let path = PathBuf::from(&raw);
    if !path.is_file() {
        return Err(format!("文件已被移动或删除：{raw}"));
    }
    Ok(path)
}

#[tauri::command]
pub async fn open_task_file(state: State<'_, Mutex<AppStore>>, id: String) -> Result<(), String> {
    let path = task_file_path(&state, &id)?;
    #[cfg(target_os = "windows")]
    {
        let target = path.to_string_lossy().to_string();
        let mut command = std::process::Command::new("cmd");
        crate::ytdlp::hide_console(&mut command);
        command
            .args(["/C", "start", "", &target])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开文件失败：{e}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开文件失败：{e}"))
    }
}

#[tauri::command]
pub async fn reveal_task_file(state: State<'_, Mutex<AppStore>>, id: String) -> Result<(), String> {
    let fallback_dir = || -> Option<String> {
        state
            .inner()
            .lock()
            .ok()
            .map(|s| s.data.settings.save_dir.clone())
    };
    let path = match task_file_path(&state, &id) {
        Ok(p) => p,
        // 老任务没有记录文件路径，退回到下载目录
        Err(_) => {
            let dir = fallback_dir().unwrap_or_else(|| ".".to_string());
            let _ = fs::create_dir_all(&dir);
            return open_with_system(Path::new(&dir));
        }
    };
    #[cfg(target_os = "windows")]
    {
        let arg = format!("/select,{}", path.display());
        std::process::Command::new("explorer")
            .arg(arg)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开文件夹失败：{e}"))
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开文件夹失败：{e}"))
    }
    #[cfg(target_os = "linux")]
    {
        let dir = path.parent().unwrap_or(Path::new("."));
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开文件夹失败：{e}"))
    }
}

#[tauri::command]
pub async fn choose_dir(
    app: AppHandle, state: State<'_, Mutex<AppStore>>,
) -> Result<Option<String>, String> {
    // Stub — 前端用原生 file input 兜住
    let _ = (app, state);
    Ok(None)
}

#[tauri::command]
pub async fn read_clipboard() -> Result<String, String> {
    arboard::Clipboard::new().and_then(|mut c| c.get_text()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_library_files(
    state: State<'_, Mutex<AppStore>>,
) -> Result<Vec<crate::library::LibraryFile>, String> {
    let dir = current_save_dir(&state)?;
    tauri::async_runtime::spawn_blocking(move || crate::library::scan_library(&dir))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_library_file(
    state: State<'_, Mutex<AppStore>>,
    path: String,
) -> Result<(), String> {
    let dir = current_save_dir(&state)?;
    let path = crate::library::validate_library_file(&dir, Path::new(&path))?;
    open_with_system(&path)
}

#[tauri::command]
pub async fn open_library_folder(
    state: State<'_, Mutex<AppStore>>,
    path: String,
) -> Result<(), String> {
    let dir = current_save_dir(&state)?;
    let path = crate::library::validate_library_file(&dir, Path::new(&path))?;
    let folder = path
        .parent()
        .ok_or_else(|| "找不到文件所在目录。".to_string())?;
    open_with_system(folder)
}

#[tauri::command]
pub async fn delete_library_file(
    state: State<'_, Mutex<AppStore>>,
    path: String,
) -> Result<(), String> {
    let dir = current_save_dir(&state)?;
    crate::library::delete_library_file(&dir, Path::new(&path))
}

#[tauri::command]
pub async fn list_subscriptions(
    state: State<'_, Mutex<AppStore>>,
) -> Result<Vec<crate::subscriptions::Subscription>, String> {
    let s = state.inner().lock().map_err(|e| e.to_string())?;
    Ok(s.data.subscriptions.clone())
}

#[tauri::command]
pub async fn add_subscription(
    state: State<'_, Mutex<AppStore>>,
    url: String,
) -> Result<crate::subscriptions::Subscription, String> {
    let mut s = state.inner().lock().map_err(|e| e.to_string())?;
    let subscription = crate::subscriptions::add(&mut s.data.subscriptions, &url, now_ms())?;
    s.save_result()?;
    Ok(subscription)
}

#[tauri::command]
pub async fn check_subscription(
    state: State<'_, Mutex<AppStore>>,
    checks: State<'_, Mutex<HashSet<String>>>,
    id: String,
) -> Result<crate::subscriptions::SubscriptionCheckResult, String> {
    {
        let mut active = checks.inner().lock().map_err(|e| e.to_string())?;
        if !active.insert(id.clone()) {
            return Err("这个订阅正在检查更新，请稍候。".into());
        }
    }

    let result = async {
        let (subscription, settings) = {
            let s = state.inner().lock().map_err(|e| e.to_string())?;
            let subscription = s
                .data
                .subscriptions
                .iter()
                .find(|item| item.id == id)
                .cloned()
                .ok_or_else(|| "订阅不存在或已删除。".to_string())?;
            (subscription, s.data.settings.clone())
        };
        let url = subscription.url.clone();
        let fetched = tauri::async_runtime::spawn_blocking(move || {
            crate::subscriptions::fetch_flat_playlist(&url, &settings)
        })
        .await
        .map_err(|e| e.to_string())??;

        let mut s = state.inner().lock().map_err(|e| e.to_string())?;
        let index = s
            .data
            .subscriptions
            .iter()
            .position(|item| item.id == id)
            .ok_or_else(|| "订阅不存在或已删除。".to_string())?;
        let result = crate::subscriptions::merge_entries(&s.data.subscriptions[index], fetched);
        s.data.subscriptions[index] = result.subscription.clone();
        s.save_result()?;
        Ok(result)
    }
    .await;

    checks
        .inner()
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&id);
    result
}

#[tauri::command]
pub async fn delete_subscription(
    state: State<'_, Mutex<AppStore>>,
    id: String,
) -> Result<(), String> {
    let mut s = state.inner().lock().map_err(|e| e.to_string())?;
    let old_len = s.data.subscriptions.len();
    s.data.subscriptions.retain(|item| item.id != id);
    if s.data.subscriptions.len() == old_len {
        return Err("订阅不存在或已删除。".into());
    }
    s.save_result()
}

#[tauri::command]
pub async fn open_builtin_browser(app: AppHandle, url: Option<String>) -> Result<(), String> {
    crate::browser::open_or_focus(&app, url.as_deref())
}

#[tauri::command]
pub async fn navigate_builtin_browser(app: AppHandle, url: String) -> Result<(), String> {
    crate::browser::navigate(&app, &url)
}

#[tauri::command]
pub async fn reload_builtin_browser(app: AppHandle) -> Result<(), String> {
    crate::browser::reload(&app)
}

#[tauri::command]
pub async fn close_builtin_browser(app: AppHandle) -> Result<(), String> {
    crate::browser::close(&app)
}

#[tauri::command]
pub async fn check_builtin_cookies(app: AppHandle, url: String) -> Result<u32, String> {
    crate::browser::cookie_count(&app, &url)
}

#[tauri::command]
pub async fn sniffer_report(
    window: tauri::WebviewWindow,
    state: State<'_, Mutex<SnifferState>>,
    url: String,
    page_url: String,
    title: String,
) -> Result<(), String> {
    crate::browser::report(&window, state.inner(), url, page_url, title)
}

/// 内置浏览器侧栏拉取**当前页面**已捕获的媒体列表
#[tauri::command]
pub async fn sniffer_list(
    state: State<'_, Mutex<SnifferState>>,
    page_url: Option<String>,
) -> Result<Vec<SnifferPayload>, String> {
    crate::browser::list(state.inner(), page_url.as_deref())
}

/// 侧栏「解析当前页面」：由后端读取浏览器当前地址，不信任页面传入的 URL
#[tauri::command]
pub async fn sniffer_enqueue_page(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    crate::browser::enqueue_page(&app, &window)
}

/// 侧栏点「添加」：仅接受已捕获列表中的地址
#[tauri::command]
pub async fn sniffer_enqueue(
    app: AppHandle,
    state: State<'_, Mutex<SnifferState>>,
    url: String,
) -> Result<(), String> {
    crate::browser::enqueue(&app, state.inner(), &url)
}

/// 侧栏点「完成登录」：按站点保存内置浏览器 Cookie 并通知主窗口重试
#[tauri::command]
pub async fn browser_login_done(app: AppHandle) -> Result<crate::ytdlp::SiteLogin, String> {
    let login = crate::browser::export_cookies(&app).await?;
    let _ = app.emit("browser:login-done", login.clone());
    Ok(login)
}

/// 设置页：列出已保存的站点登录
#[tauri::command]
pub async fn list_site_logins() -> Result<Vec<crate::ytdlp::SiteLogin>, String> {
    Ok(crate::ytdlp::list_site_logins())
}

/// 设置页：删除某个站点的登录信息
#[tauri::command]
pub async fn delete_site_login(domain: String) -> Result<(), String> {
    crate::ytdlp::delete_site_login(&domain)
}

#[tauri::command]
pub async fn submit_report(
    app: AppHandle,
    state: State<'_, Mutex<AppStore>>,
    input: crate::report::ReportInput,
) -> Result<crate::report::ReportResult, String> {
    crate::report::submit(&app, state.inner(), input).await
}

#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    state: State<'_, Mutex<AppStore>>,
) -> Result<crate::update::UpdateInfo, String> {
    crate::update::check(&app, state.inner()).await
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, Mutex<AppStore>>,
    asset_url: String,
    sha256: Option<String>,
    size: Option<u64>,
) -> Result<(), String> {
    crate::update::install(&app, state.inner(), &asset_url, sha256, size).await
}

#[tauri::command]
pub async fn open_update_log() -> Result<(), String> {
    let path = crate::update::log_path();
    if !path.exists() {
        return Err("还没有更新日志，请先尝试一次更新。".into());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开更新日志失败：{e}"))
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开更新日志失败：{e}"))
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(Path::new(".")))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开更新日志失败：{e}"))
    }
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| "链接格式不正确。".to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("只允许打开 http 或 https 链接。".into());
    }
    open_url(parsed.as_str())
}

#[cfg(test)]
mod tests {
    use super::{needs_login_error, parse_json};

    #[test]
    fn detects_login_required_failures() {
        assert!(needs_login_error(
            "ERROR: [Douyin] 123: Fresh cookies (not necessarily logged in) are needed"
        ));
        assert!(needs_login_error("ERROR: Sign in to confirm you're not a bot"));
        assert!(needs_login_error("HTTP Error 403: Forbidden"));
        assert!(!needs_login_error("ERROR: Unable to download webpage: timeout"));
    }

    #[test]
    fn parsed_media_always_keeps_a_download_url() {
        let webpage = parse_json(
            r#"{"webpage_url":"https://example.com/watch","original_url":"https://example.com/original","formats":[]}"#,
            "https://example.com/input",
        )
        .unwrap();
        assert_eq!(webpage.url, "https://example.com/watch");

        let original = parse_json(
            r#"{"original_url":"https://example.com/original","formats":[]}"#,
            "https://example.com/input",
        )
        .unwrap();
        assert_eq!(original.url, "https://example.com/original");

        let source = parse_json(r#"{"formats":[]}"#, "https://example.com/input").unwrap();
        assert_eq!(source.url, "https://example.com/input");
    }
}
