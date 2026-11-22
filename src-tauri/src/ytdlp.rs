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

/// Windows 下 GUI 程序调起控制台子进程（yt-dlp / cmd）会额外弹出一个黑色窗口，
/// 统一加上 CREATE_NO_WINDOW 隐藏它；其他平台为空操作。
pub fn hide_console(cmd: &mut std::process::Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = cmd;
}

/// 同 [`hide_console`]，用于 tokio 异步进程。
pub fn hide_console_async(cmd: &mut tokio::process::Command) {
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = cmd;
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
    pub url: String,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParseResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<MediaInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 失败原因是否是「需要登录 / Cookie」，前端据此显示「去登录」
    #[serde(default)]
    pub needs_login: bool,
}

pub const HONGGUO_USER_AGENT: &str =
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148";
pub const HONGGUO_REFERER: &str = "https://novelquickapp.com/";

#[derive(Debug)]
pub struct HongguoMedia {
    pub info: MediaInfo,
    pub play_url: String,
}

pub fn is_hongguo_url(url: &str) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|u| {
        u.host_str()
            .is_some_and(|host| host == "novelquickapp.com" || host.ends_with(".novelquickapp.com"))
    })
}

/// 红果分享页本身不被 yt-dlp 支持，但页面 pageData 中带有公开的 MP4 地址。
pub async fn resolve_hongguo(url: &str, proxy: &str) -> Result<HongguoMedia, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(HONGGUO_USER_AGENT)
        .timeout(std::time::Duration::from_secs(25));
    if !proxy.is_empty() {
        builder =
            builder.proxy(reqwest::Proxy::all(proxy).map_err(|e| format!("代理配置无效：{e}"))?);
    }
    let html = builder
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .header(reqwest::header::REFERER, HONGGUO_REFERER)
        .send()
        .await
        .map_err(|e| format!("请求红果分享页失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("红果分享页返回错误：{e}"))?
        .text()
        .await
        .map_err(|e| format!("读取红果分享页失败：{e}"))?;
    parse_hongguo_html(&html, url)
}

fn parse_hongguo_html(html: &str, source_url: &str) -> Result<HongguoMedia, String> {
    let attr = regex::Regex::new(r#"data-fn-args="([^"]+)""#).map_err(|e| e.to_string())?;
    let payload = attr.captures_iter(html).find_map(|cap| {
        let decoded = html_escape::decode_html_entities(cap.get(1)?.as_str()).into_owned();
        if !decoded.contains("series_data") || !decoded.contains("play_url") {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(&decoded).ok()
    });
    let payload = payload.ok_or_else(|| "页面中没有视频数据，链接可能已失效。".to_string())?;
    let page_data = payload
        .get(2)
        .ok_or_else(|| "视频数据格式异常。".to_string())?;
    let series = page_data
        .get("series_data")
        .ok_or_else(|| "页面中缺少视频信息。".to_string())?;
    let play_url = series
        .get("play_url")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "页面中缺少播放地址，可能需要更新解析规则。".to_string())?
        .to_string();

    let base_title = series
        .get("title")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("红果视频");
    let chapter = page_data
        .get("chapter_order")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let title = if chapter > 0 {
        format!("{base_title}（第{chapter}集）")
    } else {
        base_title.to_string()
    };
    let height = series
        .get("video_meta")
        .and_then(|v| v.get("height"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|h| u16::try_from(h).ok());
    let video_label = height
        .map(|h| format!("原画 {h}P（MP4）"))
        .unwrap_or_else(|| "原画（MP4）".into());
    let formats = vec![
        FormatOption {
            id: "source".into(),
            label: video_label,
            kind: "video".into(),
            selector: "best".into(),
            audio_format: None,
            height,
            ext: Some("mp4".into()),
        },
        FormatOption {
            id: "audio-mp3".into(),
            label: "仅下载音频 MP3".into(),
            kind: "audio".into(),
            selector: "best".into(),
            audio_format: Some("mp3".into()),
            height: None,
            ext: Some("mp3".into()),
        },
        FormatOption {
            id: "audio-m4a".into(),
            label: "仅下载音频 M4A".into(),
            kind: "audio".into(),
            selector: "best".into(),
            audio_format: Some("m4a".into()),
            height: None,
            ext: Some("m4a".into()),
        },
    ];

    Ok(HongguoMedia {
        info: MediaInfo {
            id: page_data
                .get("used_chapter_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            url: source_url.to_string(),
            title,
            thumbnail: series
                .get("series_cover")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            duration: series.get("duration").and_then(serde_json::Value::as_f64),
            uploader: None,
            site: Some("红果短剧".into()),
            formats,
        },
        play_url,
    })
}

/// 内置浏览器导出的 Cookie 文件路径（与配置文件同目录）
pub fn builtin_cookies_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("Vidgo").join("builtin-cookies.txt")
}

/// 传给 yt-dlp 的 Cookie 参数。临时副本用于避免多任务并发时 yt-dlp 回写同一文件；
/// 离开作用域自动删除。
pub struct CookieArgs {
    pub args: Vec<String>,
    temp: Option<PathBuf>,
}

impl Drop for CookieArgs {
    fn drop(&mut self) {
        if let Some(path) = self.temp.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn builtin_cookies_fresh(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < std::time::Duration::from_secs(7 * 24 * 3600))
}

/// 优先使用内置浏览器导出的 Cookie，其次回退系统浏览器。
pub fn cookie_args(settings: &crate::store::Settings) -> CookieArgs {
    let path = builtin_cookies_path();
    if builtin_cookies_fresh(&path) {
        if let Ok(bytes) = std::fs::read(&path) {
            let temp = std::env::temp_dir()
                .join(format!("vidgo-cookies-{}.txt", uuid::Uuid::new_v4()));
            if std::fs::write(&temp, bytes).is_ok() {
                return CookieArgs {
                    args: vec!["--cookies".into(), temp.to_string_lossy().to_string()],
                    temp: Some(temp),
                };
            }
        }
    }
    if !settings.cookies_from_browser.is_empty() {
        return CookieArgs {
            args: vec![
                "--cookies-from-browser".into(),
                settings.cookies_from_browser.clone(),
            ],
            temp: None,
        };
    }
    CookieArgs {
        args: Vec::new(),
        temp: None,
    }
}

pub fn build_download_args(
    url: &str,
    selector: &str,
    save_dir: &str,
    proxy: &str,
    cookie_args: &[String],
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
        "--print".into(),
        "after_move:VGFEND|%(filepath)s".into(),
    ];
    // 明确告诉 yt-dlp 用随包附带的 ffmpeg（合并 DASH 音视频 / 转 MP3 必需）
    let ff = ffmpeg_path();
    if ff.exists() {
        args.push("--ffmpeg-location".into());
        args.push(ff.to_string_lossy().to_string());
    }
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
    args.extend(cookie_args.iter().cloned());
    args.push(url.into());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hongguo_page_data() {
        let html = r#"<script data-fn-args="[&quot;video-animation-share_page&quot;,&quot;pageData&quot;,{&quot;series_data&quot;:{&quot;title&quot;:&quot;测试剧&quot;,&quot;play_url&quot;:&quot;https://v3-share.qznovel.com/video.mp4?a=1&amp;b=2&quot;,&quot;series_cover&quot;:&quot;https://img.test/cover.jpg&quot;,&quot;video_meta&quot;:{&quot;height&quot;:720}},&quot;used_chapter_id&quot;:&quot;123&quot;,&quot;chapter_order&quot;:1}]"></script>"#;
        let media = parse_hongguo_html(html, "https://novelquickapp.com/s/test/").unwrap();
        assert_eq!(media.info.id, "123");
        assert_eq!(media.info.url, "https://novelquickapp.com/s/test/");
        assert_eq!(media.info.title, "测试剧（第1集）");
        assert_eq!(media.info.formats[0].height, Some(720));
        assert_eq!(
            media.play_url,
            "https://v3-share.qznovel.com/video.mp4?a=1&b=2"
        );
    }

    #[test]
    fn only_accepts_novelquickapp_hosts() {
        assert!(is_hongguo_url("https://novelquickapp.com/s/abc/"));
        assert!(is_hongguo_url(
            "https://www.novelquickapp.com/hongguo/ug/pages/video-animation-share"
        ));
        assert!(!is_hongguo_url("https://example.com/novelquickapp.com"));
    }
}
