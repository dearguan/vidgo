use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use uuid::Uuid;

pub const BROWSER_LABEL: &str = "builtin-browser";
const DEFAULT_URL: &str = "https://www.douyin.com";
const MAX_URL_LENGTH: usize = 8 * 1024;
const MAX_TITLE_CHARS: usize = 512;
const DEDUP_WINDOW: Duration = Duration::from_secs(10);
const RATE_WINDOW: Duration = Duration::from_secs(10);
const MAX_REPORTS_PER_RATE_WINDOW: usize = 60;

const SNIFFER_SCRIPT: &str = r##"
(() => {
  const installedFlag = '__vidgoSnifferInstalledV1';
  if (window[installedFlag]) return;
  try {
    Object.defineProperty(window, installedFlag, { value: true });
  } catch (_) {
    return;
  }

  const recent = new Map();
  const dedupMs = 10000;
  const maxRecent = 512;

  function normalizeMediaUrl(candidate) {
    if (candidate == null) return null;
    if (typeof candidate !== 'string') {
      if (candidate instanceof URL) candidate = candidate.href;
      else if (typeof candidate.url === 'string') candidate = candidate.url;
      else return null;
    }

    const raw = candidate.trim();
    if (!raw || raw.length > 8192 || /^(?:blob|data):/i.test(raw)) return null;

    let parsed;
    try {
      parsed = new URL(raw, location.href);
    } catch (_) {
      return null;
    }
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return null;

    const pathname = parsed.pathname.toLowerCase();
    if (/\.(?:png|jpe?g|gif|webp|avif|svg|ico|bmp|vtt|srt|ass|ssa|ts)$/.test(pathname)) {
      return null;
    }
    if (!/\.(?:mp4|m3u8|mpd|webm|m4a|mp3)$/.test(pathname)) return null;
    return parsed.href;
  }

  function report(candidate) {
    const url = normalizeMediaUrl(candidate);
    if (!url) return;
    if (!window.__TAURI__ || !window.__TAURI__.core ||
        typeof window.__TAURI__.core.invoke !== 'function') return;

    const now = Date.now();
    const previous = recent.get(url);
    if (previous !== undefined && now - previous < dedupMs) return;

    if (recent.size >= maxRecent) {
      for (const [knownUrl, timestamp] of recent) {
        if (now - timestamp >= dedupMs) recent.delete(knownUrl);
      }
      while (recent.size >= maxRecent) {
        const oldest = recent.keys().next();
        if (oldest.done) break;
        recent.delete(oldest.value);
      }
    }
    recent.set(url, now);

    try {
      const pending = window.__TAURI__.core.invoke('sniffer_report', {
        url,
        pageUrl: location.href,
        title: document.title
      });
      if (pending && typeof pending.catch === 'function') pending.catch(() => {});
    } catch (_) {}
  }

  function inspectMediaElement(element) {
    if (!element) return;
    report(element.currentSrc);
    report(element.src);
    if (typeof element.getAttribute === 'function') report(element.getAttribute('src'));
  }

  function scanTree(root) {
    if (!root) return;
    if (root.nodeType === 1 && typeof root.matches === 'function' &&
        root.matches('video, audio, source')) {
      inspectMediaElement(root);
    }
    if (typeof root.querySelectorAll === 'function') {
      root.querySelectorAll('video, audio, source').forEach(inspectMediaElement);
    }
  }

  scanTree(document);
  const mutationObserver = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (mutation.type === 'attributes') {
        if (mutation.target.matches('video, audio, source')) {
          inspectMediaElement(mutation.target);
        }
      }
      for (const node of mutation.addedNodes) scanTree(node);
    }
  });
  mutationObserver.observe(document, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['src']
  });
  document.addEventListener('DOMContentLoaded', () => scanTree(document), { once: true });
  document.addEventListener('play', (event) => inspectMediaElement(event.target), true);
  document.addEventListener('loadedmetadata', (event) => inspectMediaElement(event.target), true);

  function scanPerformance(entries) {
    for (const entry of entries) report(entry.name);
  }
  if (window.performance && typeof performance.getEntriesByType === 'function') {
    scanPerformance(performance.getEntriesByType('resource'));
    if (typeof PerformanceObserver === 'function') {
      try {
        const performanceObserver = new PerformanceObserver((list) => {
          scanPerformance(list.getEntries());
        });
        performanceObserver.observe({ type: 'resource', buffered: true });
      } catch (_) {
        setInterval(() => scanPerformance(performance.getEntriesByType('resource')), 5000);
      }
    } else {
      setInterval(() => scanPerformance(performance.getEntriesByType('resource')), 5000);
    }
  }

  if (typeof window.fetch === 'function') {
    const originalFetch = window.fetch;
    window.fetch = function () {
      try { report(arguments[0]); } catch (_) {}
      return originalFetch.apply(this, arguments);
    };
  }

  if (window.XMLHttpRequest && XMLHttpRequest.prototype &&
      typeof XMLHttpRequest.prototype.open === 'function') {
    const originalOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function () {
      try { report(arguments[1]); } catch (_) {}
      return originalOpen.apply(this, arguments);
    };
  }
})();
"##;

#[derive(Debug, Default)]
pub struct SnifferState {
    recent_urls: HashMap<String, Instant>,
    report_times: VecDeque<Instant>,
}

impl SnifferState {
    fn accept(&mut self, url: &str, now: Instant) -> Result<(), String> {
        self.recent_urls
            .retain(|_, seen| now.duration_since(*seen) < DEDUP_WINDOW);
        while self
            .report_times
            .front()
            .is_some_and(|seen| now.duration_since(*seen) >= RATE_WINDOW)
        {
            self.report_times.pop_front();
        }

        if self.recent_urls.contains_key(url) {
            return Err("忽略 10 秒内的重复媒体地址".into());
        }
        if self.report_times.len() >= MAX_REPORTS_PER_RATE_WINDOW {
            return Err("媒体地址上报过于频繁".into());
        }

        self.recent_urls.insert(url.to_owned(), now);
        self.report_times.push_back(now);
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnifferPayload {
    pub id: String,
    pub url: String,
    pub media_type: &'static str,
    pub title: String,
    pub page_url: String,
    pub timestamp: u64,
}

pub fn open_or_focus(app: &AppHandle, url: Option<&str>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(BROWSER_LABEL) {
        if let Some(url) = url {
            window
                .navigate(parse_http_url(url)?)
                .map_err(|error| error.to_string())?;
        }
        window.show().map_err(|error| error.to_string())?;
        window.unminimize().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    let target = parse_http_url(url.unwrap_or(DEFAULT_URL))?;
    let data_directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join(BROWSER_LABEL);
    std::fs::create_dir_all(&data_directory).map_err(|error| error.to_string())?;

    WebviewWindowBuilder::new(app, BROWSER_LABEL, WebviewUrl::External(target))
        .title("Vidgo 内置浏览器")
        .inner_size(1200.0, 800.0)
        .center()
        .data_directory(data_directory)
        .initialization_script_for_all_frames(SNIFFER_SCRIPT)
        .build()
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn navigate(app: &AppHandle, url: &str) -> Result<(), String> {
    let target = parse_http_url(url)?;
    browser_window(app)?
        .navigate(target)
        .map_err(|error| error.to_string())
}

pub fn reload(app: &AppHandle) -> Result<(), String> {
    browser_window(app)?
        .reload()
        .map_err(|error| error.to_string())
}

pub fn close(app: &AppHandle) -> Result<(), String> {
    browser_window(app)?
        .close()
        .map_err(|error| error.to_string())
}

pub fn cookie_count(app: &AppHandle, url: &str) -> Result<u32, String> {
    let url = parse_http_url(url)?;
    let cookies = browser_window(app)?
        .cookies_for_url(url)
        .map_err(|error| error.to_string())?;
    u32::try_from(cookies.len()).map_err(|_| "Cookie 数量超出计数范围".into())
}

pub fn report(
    window: &WebviewWindow,
    state: &Mutex<SnifferState>,
    url: String,
    page_url: String,
    title: String,
) -> Result<(), String> {
    if window.label() != BROWSER_LABEL {
        return Err("仅内置浏览器可以上报媒体地址".into());
    }

    let (media_url, media_type) = parse_media_url(&url)?;
    let page_url = parse_http_url(&page_url)?;
    let title: String = title.trim().chars().take(MAX_TITLE_CHARS).collect();
    let now = Instant::now();

    state
        .lock()
        .map_err(|error| error.to_string())?
        .accept(media_url.as_str(), now)?;

    let payload = SnifferPayload {
        id: Uuid::new_v4().to_string(),
        url: media_url.into(),
        media_type,
        title,
        page_url: page_url.into(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default(),
    };
    window
        .app_handle()
        .emit("sniffer:found", payload)
        .map_err(|error| error.to_string())
}

fn browser_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window(BROWSER_LABEL)
        .ok_or_else(|| "内置浏览器尚未打开".into())
}

fn parse_http_url(raw: &str) -> Result<Url, String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > MAX_URL_LENGTH {
        return Err("URL 为空或超过长度限制".into());
    }
    let parsed = Url::parse(raw).map_err(|_| "URL 格式无效".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("仅支持有效的 http/https URL".into());
    }
    Ok(parsed)
}

fn parse_media_url(raw: &str) -> Result<(Url, &'static str), String> {
    let url = parse_http_url(raw)?;
    let media_type = classify_media_type(&url).ok_or_else(|| "不支持的媒体地址".to_owned())?;
    Ok((url, media_type))
}

fn classify_media_type(url: &Url) -> Option<&'static str> {
    let pathname = url.path().to_ascii_lowercase();
    let extension = pathname.rsplit_once('.')?.1;
    match extension {
        "mp4" => Some("mp4"),
        "m3u8" => Some("hls"),
        "mpd" => Some("dash"),
        "webm" => Some("webm"),
        "m4a" => Some("m4a"),
        "mp3" => Some("mp3"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_media_by_pathname_and_keeps_query() {
        let cases = [
            ("https://cdn.example.com/a/video.MP4?token=a%2Bb", "mp4"),
            ("https://cdn.example.com/live/index.m3u8?x=1", "hls"),
            ("https://cdn.example.com/live/manifest.mpd?x=1", "dash"),
            ("http://cdn.example.com/a.webm?x=1", "webm"),
            ("https://cdn.example.com/a.m4a?x=1", "m4a"),
            ("https://cdn.example.com/a.mp3?x=1", "mp3"),
        ];
        for (raw, expected) in cases {
            let parsed = parse_http_url(raw).unwrap();
            assert_eq!(classify_media_type(&parsed), Some(expected));
            assert!(parsed.query().is_some(), "query 必须保留: {raw}");
        }
    }

    #[test]
    fn filters_non_media_images_subtitles_segments_and_non_http_urls() {
        let rejected = [
            "https://example.com/a.ts?part=1",
            "https://example.com/a.mp4.ts",
            "https://example.com/poster.jpg?x=.mp4",
            "https://example.com/subtitle.vtt?x=.m3u8",
            "https://example.com/video?format=mp4",
            "https://example.com/video.mp4/extra",
            "blob:https://example.com/id.mp4",
            "data:video/mp4;base64,AAAA",
            "file:///tmp/video.mp4",
        ];
        for raw in rejected {
            assert!(parse_media_url(raw).is_err(), "不应接受: {raw}");
        }
    }

    #[test]
    fn rejects_invalid_and_overlong_navigation_urls() {
        assert!(parse_http_url("javascript:alert(1)").is_err());
        assert!(parse_http_url("https://").is_err());
        assert!(parse_http_url(&format!(
            "https://example.com/{}",
            "a".repeat(MAX_URL_LENGTH)
        ))
        .is_err());
    }

    #[test]
    fn state_deduplicates_for_ten_seconds_and_limits_rate() {
        let start = Instant::now();
        let mut state = SnifferState::default();
        state.accept("https://example.com/a.mp4", start).unwrap();
        assert!(state
            .accept("https://example.com/a.mp4", start + Duration::from_secs(9))
            .is_err());
        assert!(state
            .accept("https://example.com/a.mp4", start + DEDUP_WINDOW)
            .is_ok());

        let mut state = SnifferState::default();
        for index in 0..MAX_REPORTS_PER_RATE_WINDOW {
            state
                .accept(&format!("https://example.com/{index}.mp4"), start)
                .unwrap();
        }
        assert!(state
            .accept("https://example.com/overflow.mp4", start)
            .is_err());
        assert!(state
            .accept("https://example.com/overflow.mp4", start + RATE_WINDOW)
            .is_ok());
    }
}
