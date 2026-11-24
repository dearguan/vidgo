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
    /// 解析时是否合并了系统浏览器 Cookie（下载时需要沿用同样的来源）
    #[serde(default)]
    pub browser_cookies: bool,
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
        u.host_str().is_some_and(|host| {
            host == "novelquickapp.com"
                || host.ends_with(".novelquickapp.com")
                || host == "hainanyuyue.com"
                || host.ends_with(".hainanyuyue.com")
                || host == "kylin.hainanyuyue.com"
        })
    })
}

/// 根据链接域名取对应的 Referer（红果 / 番茄系分享页要求同站 Referer）
pub fn hongguo_referer(url: &str) -> &'static str {
    if url.contains("hainanyuyue.com") {
        "https://hainanyuyue.com/"
    } else {
        HONGGUO_REFERER
    }
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
        .header(reqwest::header::REFERER, hongguo_referer(url))
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

/// 站点 Cookie 目录：每个域名一个 Netscape 格式文件（由内置浏览器导出）
pub fn cookies_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("Vidgo").join("site-cookies")
}

/// 0.4.9 曾把 Cookie 放在 `Vidgo/cookies`；该名字在 macOS 会与 WebView 的
/// `Cookies` 数据库文件冲突，迁移到 `Vidgo/site-cookies`。
pub fn migrate_cookies_dir() {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let old = base.join("Vidgo").join("cookies");
    let new = cookies_dir();
    if old.is_dir() && !new.exists() {
        let _ = std::fs::rename(&old, &new);
    }
}

/// 国内视频站点：直连可避免代理节点导致的地区限制 / Cookie 校验失败
pub fn is_domestic_site(url: &str) -> bool {
    const DOMAINS: [&str; 22] = [
        "douyin.com",
        "douyinvod.com",
        "douyinstatic.com",
        "ixigua.com",
        "bilibili.com",
        "b23.tv",
        "hdslb.com",
        "kuaishou.com",
        "xiaohongshu.com",
        "xhslink.com",
        "weibo.com",
        "weibo.cn",
        "qq.com",
        "iqiyi.com",
        "youku.com",
        "mgtv.com",
        "sohu.com",
        "163.com",
        "novelquickapp.com",
        "qznovel.com",
        "huya.com",
        "douyu.com",
    ];
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    DOMAINS
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// 实际传给 yt-dlp / 内置浏览器的代理：国内站点按设置直连
pub fn effective_proxy(settings: &crate::store::Settings, url: &str) -> String {
    let proxy = settings.proxy.trim();
    if proxy.is_empty() {
        return String::new();
    }
    if settings.direct_cn && is_domestic_site(url) {
        return String::new();
    }
    proxy.to_string()
}

/// 清洗用户粘贴的链接：去掉首尾文字 / 引号，解码 `&amp;` 之类的 HTML 实体。
pub fn sanitize_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let start = trimmed.find("https://").or_else(|| trimmed.find("http://"))?;
    let tail = &trimmed[start..];
    let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
    let candidate = tail[..end]
        .trim_end_matches(['"', '\'', '>', '。', '，', ')', '）', ']', '}']);
    let decoded = html_escape::decode_html_entities(candidate).into_owned();
    let parsed = reqwest::Url::parse(&decoded).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(parsed.to_string())
}

/// 抖音个人主页/发现页上的 `vid` / `modal_id` 参数可直接还原成视频链接，
/// 否则 yt-dlp 会报 Unsupported URL。
pub fn canonicalize_url(raw: &str) -> String {
    let Some(sanitized) = sanitize_url(raw) else {
        return raw.trim().to_string();
    };
    let Ok(parsed) = reqwest::Url::parse(&sanitized) else {
        return sanitized;
    };
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    if host == "douyin.com" && (parsed.path().contains("/user/") || parsed.path().starts_with("/discover")) {
        let vid = parsed
            .query_pairs()
            .find(|(key, _)| key == "vid" || key == "modal_id")
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()));
        if let Some(vid) = vid {
            return format!("https://www.douyin.com/video/{vid}");
        }
    }
    sanitized
}

/// 抖音个人主页不是 yt-dlp 支持的订阅源
pub fn is_douyin_user_page(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    (host == "douyin.com" || host.ends_with(".douyin.com")) && parsed.path().starts_with("/user/")
}

/// 站点登录信息（设置页展示用）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteLogin {
    pub domain: String,
    pub cookie_count: u32,
    pub saved_at: i64,
    pub size: u64,
}

const MULTI_LEVEL_SUFFIXES: [&str; 10] = [
    "com.cn", "net.cn", "org.cn", "gov.cn", "edu.cn", "com.hk", "com.tw", "co.jp", "co.uk",
    "com.au",
];

/// 取「可注册域名」作为 Cookie 文件名：v.douyin.com -> douyin.com，
/// www.bbc.co.uk -> bbc.co.uk。IP 地址原样返回。
pub fn registrable_domain(host: &str) -> String {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    let labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    if labels.len() <= 2 {
        return labels.join(".");
    }
    let last_two = format!(
        "{}.{}",
        labels[labels.len() - 2],
        labels[labels.len() - 1]
    );
    let take = if MULTI_LEVEL_SUFFIXES.contains(&last_two.as_str()) && labels.len() >= 3 {
        3
    } else {
        2
    };
    labels[labels.len() - take..].join(".")
}

/// 分享短链的域名与主站不同，复用主站的登录 Cookie
fn alias_domain(host: &str) -> Option<&'static str> {
    match host {
        "youtu.be" => Some("youtube.com"),
        "b23.tv" => Some("bilibili.com"),
        "xhslink.com" => Some("xiaohongshu.com"),
        "v.kuaishou.com" => Some("kuaishou.com"),
        _ => None,
    }
}

fn cookie_file_for_host(host: &str) -> Option<PathBuf> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let dir = cookies_dir();
    if !dir.is_dir() {
        return None;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        let path = dir.join(format!("{host}.txt"));
        return path.is_file().then_some(path);
    }
    let parts: Vec<&str> = host.split('.').collect();
    // 从最具体到最宽泛：v.douyin.com -> douyin.com
    for start in 0..parts.len().saturating_sub(1) {
        let candidate = parts[start..].join(".");
        let path = dir.join(format!("{candidate}.txt"));
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(alias) = alias_domain(&host) {
        let path = dir.join(format!("{alias}.txt"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn cookie_file_for_url(url: &str) -> Option<PathBuf> {
    let parsed = reqwest::Url::parse(url.trim()).ok()?;
    cookie_file_for_host(parsed.host_str()?)
}

/// 直接媒体文件不需要 Cookie，跳过可避免系统浏览器 Cookie 数据库读取失败（如 Chrome 加密）。
pub fn is_direct_media_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    let path = parsed.path().to_ascii_lowercase();
    const MEDIA_EXTS: [&str; 9] = [
        "mp4", "webm", "m4a", "mp3", "mov", "mkv", "flv", "aac", "ogg",
    ];
    MEDIA_EXTS
        .iter()
        .any(|ext| path.ends_with(&format!(".{ext}")))
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

/// 仅接受 yt-dlp 认识的真实浏览器名；auto / none / 空值不传参，避免 yt-dlp 直接报错。
fn browser_cookie_source(value: &str) -> Option<&'static str> {
    const BROWSERS: [&str; 9] = [
        "chrome", "chromium", "edge", "firefox", "safari", "brave", "vivaldi", "opera", "whale",
    ];
    let lower = value.trim().to_ascii_lowercase();
    BROWSERS.iter().find(|name| **name == lower).copied()
}

/// 该链接是否有已保存的站点登录文件
pub fn has_site_cookie_file(target_url: &str) -> bool {
    cookie_file_for_url(target_url).is_some()
}

/// 把站点登录文件拼成 `Cookie` 请求头（用于自研解析器）。
/// 文件本身只包含该站点的 Cookie，这里不再按请求域名过滤，
/// 以免短链域名与主站域名不一致时漏掉登录 Cookie。
pub fn site_cookie_header(target_url: &str) -> Option<String> {
    let path = cookie_file_for_url(target_url)?;
    let text = std::fs::read_to_string(path).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut pairs = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || (line.starts_with('#') && !line.starts_with("#HttpOnly_")) {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 7 || fields[5].is_empty() {
            continue;
        }
        let expires = fields[4].parse::<i64>().unwrap_or(0);
        if expires != 0 && expires < now {
            continue;
        }
        pairs.push(format!("{}={}", fields[5], fields[6]));
    }
    (!pairs.is_empty()).then(|| pairs.join("; "))
}

pub fn is_douyin_url(url: &str) -> bool {
    reqwest::Url::parse(url.trim()).ok().is_some_and(|parsed| {
        parsed.host_str().is_some_and(|host| {
            let host = host.to_ascii_lowercase();
            host == "douyin.com"
                || host.ends_with(".douyin.com")
                || host == "iesdouyin.com"
                || host.ends_with(".iesdouyin.com")
        })
    })
}

fn douyin_id_from_url(url: &reqwest::Url) -> Option<String> {
    let pattern = regex::Regex::new(r"/(?:video|note)/(\d{8,})").ok()?;
    if let Some(captures) = pattern.captures(url.path()) {
        return Some(captures[1].to_string());
    }
    url.query_pairs()
        .find(|(key, _)| key == "vid" || key == "modal_id")
        .map(|(_, value)| value.into_owned())
        .filter(|value| value.len() >= 8 && value.chars().all(|c| c.is_ascii_digit()))
}

/// 抖音网页接口需要 `a_bogus` 签名（yt-dlp 上游已失效），但移动端详情接口
/// 只要带上内置浏览器登录后的 Cookie 就能直接返回播放地址。
pub async fn resolve_douyin(url: &str, proxy: &str) -> Result<MediaInfo, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(HONGGUO_USER_AGENT)
        .timeout(std::time::Duration::from_secs(25));
    if !proxy.is_empty() {
        builder = builder
            .proxy(reqwest::Proxy::all(proxy).map_err(|e| format!("代理配置无效：{e}"))?);
    }
    let client = builder.build().map_err(|e| e.to_string())?;

    let parsed = reqwest::Url::parse(url.trim()).map_err(|e| format!("链接无效：{e}"))?;
    let id = match douyin_id_from_url(&parsed) {
        Some(id) => id,
        None => {
            // v.douyin.com 短链：跟随跳转后从最终地址里取视频 ID
            let response = client
                .get(parsed)
                .header(reqwest::header::REFERER, "https://www.douyin.com/")
                .send()
                .await
                .map_err(|e| format!("打开抖音链接失败：{e}"))?;
            douyin_id_from_url(response.url())
                .ok_or_else(|| "无法从链接中识别抖音视频，请改用视频页链接。".to_string())?
        }
    };

    let api = format!(
        "https://aweme.snssdk.com/aweme/v1/aweme/detail/?aweme_id={id}&aid=1128&device_platform=iphone"
    );
    let mut request = client
        .get(&api)
        .header(reqwest::header::REFERER, "https://www.douyin.com/");
    if let Some(cookie) = site_cookie_header(url) {
        request = request.header(reqwest::header::COOKIE, cookie);
    }
    let payload: serde_json::Value = request
        .send()
        .await
        .map_err(|e| format!("请求抖音接口失败：{e}"))?
        .json()
        .await
        .map_err(|e| format!("抖音接口返回异常：{e}"))?;

    if let Some(code) = payload.get("status_code").and_then(serde_json::Value::as_i64) {
        if code != 0 {
            let message = payload
                .get("status_msg")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("未知错误");
            return Err(format!("抖音接口返回错误（{code}）：{message}"));
        }
    }
    let detail = payload
        .get("aweme_detail")
        .filter(|value| !value.is_null())
        .ok_or_else(|| "抖音没有返回视频信息，请确认已在「设置 → 站点登录」登录抖音。".to_string())?;

    let text = |pointer: &str| {
        detail
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let play_url = text("/video/play_addr/url_list/0")
        .or_else(|| text("/video/bit_rate/0/play_addr/url_list/0"))
        .or_else(|| text("/video/download_addr/url_list/0"))
        .ok_or_else(|| "抖音没有返回可下载地址，视频可能已删除或需要登录。".to_string())?;
    let height = detail
        .pointer("/video/height")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    let title = text("/desc").unwrap_or_else(|| "抖音视频".into());
    let video_label = height
        .map(|height| format!("原画 {height}P（MP4）"))
        .unwrap_or_else(|| "原画（MP4）".into());

    Ok(MediaInfo {
        id,
        url: play_url,
        title,
        thumbnail: text("/video/cover/url_list/0")
            .or_else(|| text("/video/origin_cover/url_list/0")),
        duration: detail
            .pointer("/video/duration")
            .and_then(serde_json::Value::as_f64)
            .map(|ms| ms / 1000.0),
        uploader: text("/author/nickname"),
        site: Some("抖音".into()),
        formats: vec![
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
        ],
    })
}

/// 是否配置了可从系统浏览器读取 Cookie
pub fn has_browser_cookie_source(settings: &crate::store::Settings) -> bool {
    browser_cookie_source(&settings.cookies_from_browser).is_some()
}

fn site_cookie_file_args(target_url: &str) -> (Vec<String>, Option<PathBuf>) {
    let Some(path) = cookie_file_for_url(target_url) else {
        return (Vec::new(), None);
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return (Vec::new(), None);
    };
    let temp = std::env::temp_dir().join(format!("vidgo-cookies-{}.txt", uuid::Uuid::new_v4()));
    if std::fs::write(&temp, bytes).is_err() {
        return (Vec::new(), None);
    }
    (
        vec!["--cookies".into(), temp.to_string_lossy().to_string()],
        Some(temp),
    )
}

/// 站点登录 Cookie。**不包含系统浏览器**：系统浏览器的 Cookie 数据库在 Windows
/// 上经常因为浏览器正在运行而无法复制，若默认带上会让所有不需要登录的站点一起失败。
pub fn cookie_args(_settings: &crate::store::Settings, target_url: &str) -> CookieArgs {
    if is_direct_media_url(target_url) {
        return CookieArgs {
            args: Vec::new(),
            temp: None,
        };
    }
    let (args, temp) = site_cookie_file_args(target_url);
    CookieArgs { args, temp }
}

/// 只使用系统浏览器 Cookie（站点没有内置登录时，作为登录兜底）。
/// 直接媒体 URL 不需要 Cookie，跳过可避免系统浏览器 Cookie 数据库读取失败。
pub fn cookie_args_browser(settings: &crate::store::Settings, target_url: &str) -> CookieArgs {
    if is_direct_media_url(target_url) {
        return CookieArgs {
            args: Vec::new(),
            temp: None,
        };
    }
    match browser_cookie_source(&settings.cookies_from_browser) {
        Some(browser) => CookieArgs {
            args: vec!["--cookies-from-browser".into(), browser.to_string()],
            temp: None,
        },
        None => CookieArgs {
            args: Vec::new(),
            temp: None,
        },
    }
}

/// 同时使用站点 Cookie 与系统浏览器 Cookie（yt-dlp 会合并两个来源）。
/// 站点登录不完整（例如抖音缺少 s_v_web_id）时作为兜底。
pub fn cookie_args_merged(settings: &crate::store::Settings, target_url: &str) -> CookieArgs {
    if is_direct_media_url(target_url) {
        return CookieArgs {
            args: Vec::new(),
            temp: None,
        };
    }
    let (mut args, temp) = site_cookie_file_args(target_url);
    if let Some(browser) = browser_cookie_source(&settings.cookies_from_browser) {
        args.push("--cookies-from-browser".into());
        args.push(browser.to_string());
    }
    CookieArgs { args, temp }
}

/// 系统浏览器 Cookie 读取失败（浏览器运行中 / 新版加密 / 未安装）。
/// 这类错误不应让「不需要 Cookie 的站点」一起失败，调用方应去掉浏览器来源重试。
pub fn is_browser_cookie_error(detail: &str) -> bool {
    let lower = detail.to_lowercase();
    [
        "could not copy chrome cookie database",
        "cookie database",
        "failed to decrypt",
        "could not decrypt",
        "could not find",
        "cookie load",
    ]
    .iter()
    .any(|key| lower.contains(key))
}

fn normalized_domain(raw: &str) -> Result<String, String> {
    let domain = raw.trim().trim_start_matches('.').to_ascii_lowercase();
    let valid = !domain.is_empty()
        && domain.len() <= 253
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && !domain.contains("..");
    if valid {
        Ok(domain)
    } else {
        Err("域名格式不正确。".into())
    }
}

/// 保存某个站点的 Cookie（覆盖旧文件）
pub fn save_site_cookies(domain: &str, text: &str) -> Result<PathBuf, String> {
    let domain = normalized_domain(domain)?;
    let dir = cookies_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 Cookie 目录失败：{e}"))?;
    let path = dir.join(format!("{domain}.txt"));
    std::fs::write(&path, text).map_err(|e| format!("保存 Cookie 失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

/// 统计 Netscape Cookie 行数（`#HttpOnly_` 前缀是真实 Cookie，不算注释）
fn count_cookie_lines(text: &str) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && (!line.starts_with('#') || line.starts_with("#HttpOnly_")))
        .count()
}

pub fn list_site_logins() -> Vec<SiteLogin> {
    let dir = cookies_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut logins: Vec<SiteLogin> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("txt") {
                return None;
            }
            let domain = path.file_stem()?.to_str()?.to_ascii_lowercase();
            let text = std::fs::read_to_string(&path).ok()?;
            let meta = entry.metadata().ok();
            let saved_at = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Some(SiteLogin {
                domain,
                cookie_count: u32::try_from(count_cookie_lines(&text)).unwrap_or(u32::MAX),
                saved_at,
                size: meta.map(|m| m.len()).unwrap_or(0),
            })
        })
        .filter(|login| login.cookie_count > 0)
        .collect();
    logins.sort_by_key(|a| std::cmp::Reverse(a.saved_at));
    logins
}

/// 查询某个链接会用到哪个站点的登录信息
pub fn site_login_for(url: &str) -> Option<SiteLogin> {
    let path = cookie_file_for_url(url)?;
    let text = std::fs::read_to_string(&path).ok()?;
    let meta = std::fs::metadata(&path).ok();
    let saved_at = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some(SiteLogin {
        domain: path.file_stem()?.to_str()?.to_string(),
        cookie_count: u32::try_from(count_cookie_lines(&text)).unwrap_or(u32::MAX),
        saved_at,
        size: meta.map(|m| m.len()).unwrap_or(0),
    })
}

pub fn delete_site_login(domain: &str) -> Result<(), String> {
    let domain = normalized_domain(domain)?;
    let path = cookies_dir().join(format!("{domain}.txt"));
    if !path.is_file() {
        return Err("该站点的登录信息不存在。".into());
    }
    std::fs::remove_file(&path).map_err(|e| format!("删除登录信息失败：{e}"))
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
    if height.is_some() {
        // 用 remux 而不是 merge-output-format：后者在只有 WebM/VP9 流时会让合并直接失败，
        // remux 失败也只是保留原容器，不影响下载成功。
        args.push("--remux-video".into());
        args.push("mp4".into());
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

    #[test]
    fn derives_registrable_domain_for_cookie_files() {
        assert_eq!(registrable_domain("v.douyin.com"), "douyin.com");
        assert_eq!(registrable_domain("WWW.Bilibili.com"), "bilibili.com");
        assert_eq!(registrable_domain("bilibili.com"), "bilibili.com");
        assert_eq!(registrable_domain("www.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(registrable_domain("a.b.example.com.cn"), "example.com.cn");
        assert_eq!(registrable_domain("127.0.0.1"), "127.0.0.1");
    }

    #[test]
    fn direct_media_urls_skip_cookies() {
        assert!(is_direct_media_url(
            "https://lf-douyin-pc-web.douyinstatic.com/obj/douyin-pc-web/uuu_265.mp4"
        ));
        assert!(!is_direct_media_url("https://v.douyin.com/zXGnY0-vJD4/"));
        assert!(!is_direct_media_url("https://www.bilibili.com/video/BV1xx"));
    }

    #[test]
    fn ignores_placeholder_browser_cookie_sources() {
        assert_eq!(browser_cookie_source("edge"), Some("edge"));
        assert_eq!(browser_cookie_source(" Chrome "), Some("chrome"));
        assert_eq!(browser_cookie_source("auto"), None);
        assert_eq!(browser_cookie_source("none"), None);
        assert_eq!(browser_cookie_source(""), None);
    }

    #[test]
    fn sanitizes_pasted_links_and_decodes_html_entities() {
        assert_eq!(
            sanitize_url("看看这个 https://v.douyin.com/abc/ 很好").as_deref(),
            Some("https://v.douyin.com/abc/")
        );
        assert_eq!(
            sanitize_url(
                "https://www.douyin.com/user/MS4?from_tab_name=main&amp;vid=7680497115004505396"
            )
            .as_deref(),
            Some("https://www.douyin.com/user/MS4?from_tab_name=main&vid=7680497115004505396")
        );
        assert_eq!(sanitize_url("没有链接"), None);
    }

    #[test]
    fn rewrites_douyin_video_params_to_canonical_url() {
        assert_eq!(
            canonicalize_url(
                "https://www.douyin.com/user/MS4?from_tab_name=main&amp;vid=7680497115004505396"
            ),
            "https://www.douyin.com/video/7680497115004505396"
        );
        assert_eq!(
            canonicalize_url("https://www.douyin.com/discover?modal_id=123"),
            "https://www.douyin.com/video/123"
        );
        assert!(is_douyin_user_page("https://www.douyin.com/user/MS4"));
        assert!(!is_douyin_user_page("https://www.douyin.com/video/123"));
    }

    #[test]
    fn domestic_sites_skip_proxy() {
        let mut settings = crate::store::Settings {
            proxy: "http://127.0.0.1:7880".into(),
            ..Default::default()
        };
        assert_eq!(effective_proxy(&settings, "https://v.douyin.com/x/"), "");
        assert_eq!(
            effective_proxy(&settings, "https://www.youtube.com/watch?v=1"),
            "http://127.0.0.1:7880"
        );
        settings.direct_cn = false;
        assert_eq!(
            effective_proxy(&settings, "https://v.douyin.com/x/"),
            "http://127.0.0.1:7880"
        );
    }

    #[test]
    fn detects_browser_cookie_read_failures() {
        assert!(is_browser_cookie_error(
            "ERROR: Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271"
        ));
        assert!(is_browser_cookie_error("ERROR: failed to decrypt cookie value"));
        assert!(!is_browser_cookie_error(
            "ERROR: [Douyin] 123: Fresh cookies (not necessarily logged in) are needed"
        ));
    }

    #[test]
    fn browser_cookie_args_only_for_real_browsers() {
        let settings = crate::store::Settings {
            cookies_from_browser: "edge".into(),
            ..Default::default()
        };
        assert_eq!(
            cookie_args_browser(&settings, "https://www.bilibili.com/video/BV1").args,
            vec!["--cookies-from-browser".to_string(), "edge".to_string()]
        );
        let settings = crate::store::Settings {
            cookies_from_browser: "auto".into(),
            ..Default::default()
        };
        assert!(cookie_args_browser(&settings, "https://www.bilibili.com/video/BV1")
            .args
            .is_empty());
        // 直接媒体 URL 不使用系统浏览器 Cookie
        let settings = crate::store::Settings {
            cookies_from_browser: "edge".into(),
            ..Default::default()
        };
        assert!(cookie_args_browser(&settings, "https://cdn.example.com/a.mp4").args.is_empty());
    }

    #[test]
    fn detects_douyin_links_and_ids() {
        assert!(is_douyin_url("https://v.douyin.com/8j4yabLB-dk/"));
        assert!(is_douyin_url(
            "https://www.douyin.com/user/x?vid=7680497115004505396"
        ));
        assert!(!is_douyin_url("https://www.bilibili.com/video/BV1xx"));
        let url = reqwest::Url::parse("https://www.iesdouyin.com/share/video/7680497115004505396/").unwrap();
        assert_eq!(douyin_id_from_url(&url).as_deref(), Some("7680497115004505396"));
    }

    /// 需要真实网络 + 已登录的抖音 Cookie：`cargo test douyin_live -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn douyin_live() {
        let info = resolve_douyin("https://v.douyin.com/8j4yabLB-dk/", "")
            .await
            .expect("解析失败");
        assert!(info.url.starts_with("http"));
        assert!(!info.title.is_empty());
        println!("{} | {} | {}", info.title, info.id, info.url);
    }

    #[test]
    fn share_links_reuse_main_site_cookies() {
        assert_eq!(alias_domain("youtu.be"), Some("youtube.com"));
        assert_eq!(alias_domain("b23.tv"), Some("bilibili.com"));
        assert_eq!(alias_domain("example.com"), None);
    }

    #[test]
    fn counts_netscape_cookies_including_httponly() {
        let text = "# Netscape HTTP Cookie File\n# Generated by Vidgo\n\
                    .douyin.com\tTRUE\t/\tTRUE\t0\tsessionid\tabc\n\
                    #HttpOnly_.douyin.com\tTRUE\t/\tTRUE\t0\ttoken\txyz\n";
        assert_eq!(count_cookie_lines(text), 2);
    }
}
