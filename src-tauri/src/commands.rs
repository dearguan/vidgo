use crate::browser::SnifferState;
use crate::store::{AppStore, DownloadTask, Settings, TaskStatus};
use crate::ytdlp::{self, FormatOption, MediaInfo, ParseResult, ytdlp_path};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
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
    std::process::Command::new(ytdlp_path())
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
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
    Ok(s.data.tasks.clone())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Mutex<AppStore>>) -> Result<Settings, String> {
    let s = state.inner().lock().map_err(|e| e.to_string())?;
    Ok(s.data.settings.clone())
}

#[tauri::command]
pub async fn set_settings(
    state: State<'_, Mutex<AppStore>>,
    new_settings: Settings,
) -> Result<Settings, String> {
    let mut s = state.inner().lock().map_err(|e| e.to_string())?;
    s.data.settings = new_settings.clone();
    s.save();
    Ok(new_settings)
}

#[tauri::command]
pub async fn parse_media(state: State<'_, Mutex<AppStore>>, url: String) -> Result<ParseResult, String> {
    let settings = {
        let s = state.inner().lock().unwrap();
        s.data.settings.clone()
    };

    if ytdlp::is_hongguo_url(&url) {
        return Ok(match ytdlp::resolve_hongguo(&url, &settings.proxy).await {
            Ok(media) => ParseResult {
                ok: true,
                info: Some(media.info),
                message: None,
                detail: None,
            },
            Err(e) => ParseResult {
                ok: false,
                info: None,
                message: Some("红果视频解析失败，请确认链接有效或稍后重试。".into()),
                detail: Some(e),
            },
        });
    }

    let mut args: Vec<String> = vec![
        "-J".into(), "--no-playlist".into(), "--no-warnings".into(),
        "--socket-timeout".into(), "20".into(),
    ];
    if !settings.proxy.is_empty() {
        args.push("--proxy".into());
        args.push(settings.proxy.clone());
    }
    if !settings.cookies_from_browser.is_empty() {
        args.push("--cookies-from-browser".into());
        args.push(settings.cookies_from_browser.clone());
    }
    args.push(url.clone());

    let output = match std::process::Command::new(ytdlp_path()).args(&args).output() {
        Ok(o) => o,
        Err(e) => {
            return Ok(ParseResult {
                ok: false,
                info: None,
                message: Some(format!("无法启动解析引擎：{}", e)),
                detail: Some(e.to_string()),
            });
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Ok(ParseResult {
            ok: false,
            info: None,
            message: Some(humanize_err(&stderr)),
            detail: Some(stderr),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(match parse_json(&stdout, &url) {
        Ok(info) => ParseResult { ok: true, info: Some(info), message: None, detail: None },
        Err(e) => ParseResult {
            ok: false, info: None,
            message: Some("解析结果异常，请稍后重试。".into()),
            detail: Some(e.to_string()),
        },
    })
}

fn humanize_err(detail: &str) -> String {
    let rules: Vec<(&str, &str)> = vec![
        ("Unsupported URL|no suitable info extractor", "当前网站暂时无法解析。可能是网站更新导致解析器失效。"),
        ("Sign in to confirm|login required|Private video", "该视频需要登录才能访问，请配置 Cookie 或用内置浏览器登录。"),
        ("HTTP Error 403|Forbidden", "网站拒绝访问（403）。可能是地区限制或需要登录。"),
        ("HTTP Error 404|not found", "视频不存在或已被删除（404）。"),
        ("getaddrinfo|timed out|Connection refused|SSL", "网络连接失败，请检查网络或代理设置。"),
        ("Fresh cookies", "该视频需要新鲜 Cookie 才能解析，请到设置里配置或用内置浏览器登录一次。"),
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
        let mut download_url = task_clone.url.clone();
        if is_hongguo {
            match ytdlp::resolve_hongguo(&task_clone.url, &settings.proxy).await {
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
        }
        let mut args = ytdlp::build_download_args(
            &download_url, &task_clone.selector, &task_clone.save_dir,
            &settings.proxy, &settings.cookies_from_browser,
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
        let child = match Command::new(ytdlp_path()).args(&args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
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
        {
            let mut s = state.inner().lock().unwrap();
            if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id2) {
                t.status = TaskStatus::Downloading;
            }
            s.save();
            let _ = app2.emit("tasks:changed", s.data.tasks.clone());
        }

        match child.wait_with_output().await {
            Ok(o) if o.status.success() => {
                let mut s = state.inner().lock().unwrap();
                if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id2) {
                    t.status = TaskStatus::Completed;
                    t.progress = 100.0;
                    t.completed_at = Some(now_ms());
                    t.speed = 0.0;
                }
                s.save();
                let _ = app2.emit("tasks:changed", s.data.tasks.clone());
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                let mut s = state.inner().lock().unwrap();
                if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id2) {
                    t.status = TaskStatus::Failed;
                    t.error = Some(humanize_err(&stderr));
                    t.error_detail = Some(stderr);
                }
                s.save();
                let _ = app2.emit("tasks:changed", s.data.tasks.clone());
            }
            Err(e) => {
                let mut s = state.inner().lock().unwrap();
                if let Some(t) = s.data.tasks.iter_mut().find(|t| t.id == id2) {
                    t.status = TaskStatus::Failed;
                    t.error = Some(e.to_string());
                }
                s.save();
                let _ = app2.emit("tasks:changed", s.data.tasks.clone());
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
    #[cfg(target_os = "windows")] { let _ = std::process::Command::new("explorer").arg(&dir).spawn(); }
    #[cfg(target_os = "macos")] { let _ = std::process::Command::new("open").arg(&dir).spawn(); }
    Ok(())
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
            crate::subscriptions::fetch_flat_playlist(
                &url,
                &settings.proxy,
                &settings.cookies_from_browser,
            )
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

#[cfg(test)]
mod tests {
    use super::parse_json;

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
