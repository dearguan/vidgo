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
const MAX_MEDIA_ITEMS: usize = 200;

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

// ---------- Vidgo 媒体侧栏（仅顶层页面） ----------
(() => {
  if (window.top !== window) return;
  const tauri = window.__TAURI__;
  if (!tauri || !tauri.core || typeof tauri.core.invoke !== 'function') return;
  const invoke = (cmd, args) => tauri.core.invoke(cmd, args);

  const HOST_ID = '__vidgo_sidebar_v1__';
  const CSS = [
    '.vg-panel{position:fixed;top:0;right:0;width:320px;height:100vh;box-sizing:border-box;',
    'background:#141720;color:#e8eaf0;font:13px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;',
    'display:flex;flex-direction:column;box-shadow:-6px 0 20px rgba(0,0,0,.35);border-left:1px solid #262a36}',
    '.vg-head{display:flex;align-items:center;gap:8px;padding:10px 12px;border-bottom:1px solid #262a36}',
    '.vg-title{flex:1;font-weight:600}',
    '.vg-btn{cursor:pointer;border:0;border-radius:6px;font:inherit;padding:5px 10px}',
    '.vg-login{background:#3b82f6;color:#fff}',
    '.vg-toggle{background:#262a36;color:#cfd4e0}',
    '.vg-status{padding:6px 12px;color:#7fd1a0;min-height:16px;font-size:12px}',
    '.vg-list{flex:1;overflow:auto}',
    '.vg-item{padding:9px 12px;border-bottom:1px solid #1e2230}',
    '.vg-badge{display:inline-block;background:#1e2a45;color:#8fb7ff;border-radius:4px;padding:0 5px;font-size:11px;margin-right:6px}',
    '.vg-name{font-weight:600;word-break:break-all}',
    '.vg-url{color:#7d8496;font-size:11px;word-break:break-all;margin:3px 0 6px}',
    '.vg-add{background:#22c55e;color:#06240f;padding:4px 10px}',
    '.vg-empty{color:#7d8496;padding:24px 12px;text-align:center}',
    '.vg-capsule{position:fixed;right:16px;bottom:18px;background:#3b82f6;color:#fff;border:0;border-radius:999px;',
    'padding:7px 12px;font:13px/1.2 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;cursor:grab;',
    'user-select:none;touch-action:none;',
    'box-shadow:0 2px 10px rgba(0,0,0,.4)}'
  ].join('');

  let host = null;
  let panelEl = null;
  let listEl = null;
  let countEl = null;
  let statusEl = null;
  let capsuleEl = null;
  let collapsed = false;
  let latest = [];
  const added = Object.create(null);

  function make(tag, cls, text) {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text != null) node.textContent = String(text);
    return node;
  }

  function setCollapsed(next) {
    collapsed = next;
    if (panelEl) panelEl.style.display = next ? 'none' : 'flex';
    if (capsuleEl) capsuleEl.style.display = next ? 'block' : 'none';
  }

  function render() {
    if (!listEl) return;
    const label = '媒体 (' + latest.length + ')';
    countEl.textContent = label;
    capsuleEl.textContent = label;
    listEl.textContent = '';

    // 当前页面固定入口：即使没捕获到独立媒体地址，也能直接交给主窗口解析
    const pageCard = make('div', 'vg-item');
    const pageTop = make('div');
    pageTop.appendChild(make('span', 'vg-badge', 'PAGE'));
    pageTop.appendChild(make('span', 'vg-name', document.title || location.hostname || '当前页面'));
    const pageUrlLine = make('div', 'vg-url', location.href);
    const pageBtn = make('button', 'vg-btn vg-add', '解析当前页面');
    pageBtn.addEventListener('click', () => {
      pageBtn.disabled = true;
      pageBtn.textContent = '已发送…';
      Promise.resolve(invoke('sniffer_enqueue_page')).then(() => {
        if (statusEl) statusEl.textContent = '已将当前页面交给主窗口解析';
      }).catch((error) => {
        pageBtn.disabled = false;
        pageBtn.textContent = '解析当前页面';
        if (statusEl) statusEl.textContent = '发送失败：' + (error && error.message ? error.message : error);
      });
    });
    pageCard.appendChild(pageTop);
    pageCard.appendChild(pageUrlLine);
    pageCard.appendChild(pageBtn);
    listEl.appendChild(pageCard);

    if (!latest.length) {
      listEl.appendChild(make('div', 'vg-empty', '没有捕获到独立媒体地址，可直接点上面的「解析当前页面」'));
      return;
    }
    for (const item of latest) {
      const wrap = make('div', 'vg-item');
      const top = make('div');
      top.appendChild(make('span', 'vg-badge', String(item.mediaType || 'media').toUpperCase()));
      top.appendChild(make('span', 'vg-name', item.title || '未命名媒体'));
      const urlLine = make('div', 'vg-url', item.url);
      const add = make('button', 'vg-btn vg-add', added[item.url] ? '已添加' : '添加');
      if (added[item.url]) add.disabled = true;
      add.addEventListener('click', () => {
        add.disabled = true;
        add.textContent = '添加中…';
        Promise.resolve(invoke('sniffer_enqueue', { url: item.url })).then(() => {
          added[item.url] = true;
          add.textContent = '已添加';
          if (statusEl) statusEl.textContent = '已加入主窗口下载列表';
        }).catch((error) => {
          add.disabled = false;
          add.textContent = '添加';
          if (statusEl) statusEl.textContent = '添加失败：' + (error && error.message ? error.message : error);
        });
      });
      wrap.appendChild(top);
      wrap.appendChild(urlLine);
      wrap.appendChild(add);
      listEl.appendChild(wrap);
    }
  }

  function build() {
    if (!document.documentElement) return;
    host = document.createElement('div');
    host.id = HOST_ID;
    host.style.position = 'fixed';
    host.style.top = '0';
    host.style.left = '0';
    host.style.width = '0';
    host.style.height = '0';
    host.style.zIndex = '2147483647';

    const root = host.attachShadow({ mode: 'open' });
    const style = document.createElement('style');
    style.textContent = CSS;
    root.appendChild(style);

    const panel = make('div', 'vg-panel');
    const head = make('div', 'vg-head');
    const title = make('span', 'vg-title', '媒体 (0)');
    const login = make('button', 'vg-btn vg-login', '完成登录');
    const toggle = make('button', 'vg-btn vg-toggle', '收起');
    head.appendChild(title);
    head.appendChild(login);
    head.appendChild(toggle);

    const status = make('div', 'vg-status', '');
    const list = make('div', 'vg-list');
    panel.appendChild(head);
    panel.appendChild(status);
    panel.appendChild(list);

    const capsule = make('button', 'vg-capsule', '媒体 (0)');
    capsule.style.display = 'none';

    root.appendChild(panel);
    root.appendChild(capsule);
    document.documentElement.appendChild(host);

    panelEl = panel;
    listEl = list;
    countEl = title;
    statusEl = status;
    capsuleEl = capsule;

    login.addEventListener('click', () => {
      status.textContent = '正在保存登录状态…';
      Promise.resolve(invoke('browser_login_done')).then((info) => {
        const detail = info && info.domain
          ? '（' + info.domain + '，' + info.cookieCount + ' 个 Cookie）'
          : '';
        status.textContent = '已保存登录状态' + detail;
      }).catch((error) => {
        status.textContent = '保存失败：' + (error && error.message ? error.message : error);
      });
    });
    toggle.addEventListener('click', () => setCollapsed(!collapsed));

    // 胶囊可拖动，避免挡住网站自带的头像 / 登录按钮；位置记在本站 localStorage
    const POS_KEY = '__vidgo_capsule_pos__';
    let dragging = false;
    let moved = false;
    let suppressClick = false;
    let startX = 0;
    let startY = 0;
    let startRight = 16;
    let startBottom = 18;
    try {
      const saved = JSON.parse(localStorage.getItem(POS_KEY) || 'null');
      if (saved && typeof saved.right === 'number' && typeof saved.bottom === 'number') {
        capsule.style.right = saved.right + 'px';
        capsule.style.bottom = saved.bottom + 'px';
      }
    } catch (_) {}
    capsule.addEventListener('pointerdown', (event) => {
      if (event.button !== 0) return;
      dragging = true;
      moved = false;
      const rect = capsule.getBoundingClientRect();
      startX = event.clientX;
      startY = event.clientY;
      startRight = window.innerWidth - rect.right;
      startBottom = window.innerHeight - rect.bottom;
      if (capsule.setPointerCapture) capsule.setPointerCapture(event.pointerId);
      capsule.style.cursor = 'grabbing';
    });
    capsule.addEventListener('pointermove', (event) => {
      if (!dragging) return;
      const dx = event.clientX - startX;
      const dy = event.clientY - startY;
      if (!moved && Math.abs(dx) < 4 && Math.abs(dy) < 4) return;
      moved = true;
      const right = Math.min(Math.max(8, startRight - dx), window.innerWidth - 90);
      const bottom = Math.min(Math.max(8, startBottom - dy), window.innerHeight - 44);
      capsule.style.right = right + 'px';
      capsule.style.bottom = bottom + 'px';
    });
    const endDrag = (event) => {
      if (!dragging) return;
      dragging = false;
      capsule.style.cursor = 'grab';
      if (capsule.releasePointerCapture) {
        try { capsule.releasePointerCapture(event.pointerId); } catch (_) {}
      }
      if (moved) {
        suppressClick = true;
        try {
          localStorage.setItem(POS_KEY, JSON.stringify({
            right: parseFloat(capsule.style.right) || 16,
            bottom: parseFloat(capsule.style.bottom) || 18
          }));
        } catch (_) {}
      }
    };
    capsule.addEventListener('pointerup', endDrag);
    capsule.addEventListener('pointercancel', endDrag);
    capsule.addEventListener('click', () => {
      if (suppressClick) {
        suppressClick = false;
        return;
      }
      setCollapsed(false);
    });

    setCollapsed(collapsed);
    render();
  }

  let lastPage = '';
  let lastJson = '';

  function refresh() {
    Promise.resolve(invoke('sniffer_list', { pageUrl: location.href })).then((items) => {
      const list = Array.isArray(items) ? items : [];
      const json = JSON.stringify(list);
      const pageChanged = location.href !== lastPage;
      if (pageChanged) lastPage = location.href;
      if (json !== lastJson || pageChanged) {
        lastJson = json;
        latest = list;
        render();
      }
    }).catch(() => {});
  }

  function tick() {
    if (!document.documentElement) return;
    if (!host || !host.isConnected) build();
    refresh();
  }

  if (document.documentElement) {
    tick();
  } else {
    document.addEventListener('DOMContentLoaded', tick, { once: true });
  }
  setInterval(tick, 2000);
})();
"##;

#[derive(Debug, Default)]
pub struct SnifferState {
    recent_urls: HashMap<String, Instant>,
    report_times: VecDeque<Instant>,
    /// 已捕获的媒体列表（新的在前），供侧栏展示与「添加」
    media: Vec<SnifferPayload>,
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

/// 当前内置浏览器实例使用的代理，用于在代理设置或目标站点变化时重建窗口
static BROWSER_PROXY: Mutex<Option<String>> = Mutex::new(None);

fn desired_browser_proxy(app: &AppHandle, target: &Url) -> String {
    let Some(state) = app.try_state::<Mutex<crate::store::AppStore>>() else {
        return String::new();
    };
    let Ok(store) = state.lock() else {
        return String::new();
    };
    crate::ytdlp::effective_proxy(&store.data.settings, target.as_str())
}

pub fn open_or_focus(app: &AppHandle, url: Option<&str>) -> Result<(), String> {
    let target = parse_http_url(url.unwrap_or(DEFAULT_URL))?;
    let proxy = desired_browser_proxy(app, &target);
    let applied = BROWSER_PROXY
        .lock()
        .map(|value| value.clone().unwrap_or_default())
        .unwrap_or_default();

    if app.get_webview_window(BROWSER_LABEL).is_some() && applied != proxy {
        // WebView 的代理只能在创建时指定：代理设置变化时重建窗口（Cookie 保存在数据目录中）
        if let Some(window) = app.get_webview_window(BROWSER_LABEL) {
            let _ = window.close();
        }
        for _ in 0..20 {
            if app.get_webview_window(BROWSER_LABEL).is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    if let Some(window) = app.get_webview_window(BROWSER_LABEL) {
        window
            .navigate(target)
            .map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window.unminimize().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    let data_directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join(BROWSER_LABEL);
    std::fs::create_dir_all(&data_directory).map_err(|error| error.to_string())?;

    let mut builder = WebviewWindowBuilder::new(app, BROWSER_LABEL, WebviewUrl::External(target.clone()))
        .title("Vidgo 内置浏览器")
        .inner_size(1200.0, 800.0)
        .center()
        .data_directory(data_directory)
        .initialization_script_for_all_frames(SNIFFER_SCRIPT);
    if !proxy.is_empty() {
        let proxy_url = Url::parse(&proxy).map_err(|error| format!("代理地址无效：{error}"))?;
        builder = builder.proxy_url(proxy_url);
    }
    builder.build().map_err(|error| error.to_string())?;
    if let Ok(mut current) = BROWSER_PROXY.lock() {
        *current = Some(proxy);
    }
    Ok(())
}

pub fn navigate(app: &AppHandle, url: &str) -> Result<(), String> {
    // 走统一的代理判定：从国内站点跳到境外站点时需要重建窗口
    open_or_focus(app, Some(url))
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

    let payload = SnifferPayload {
        id: Uuid::new_v4().to_string(),
        url: media_url.to_string(),
        media_type,
        title,
        page_url: page_url.to_string(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default(),
    };
    {
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        guard.accept(payload.url.as_str(), now)?;
        guard.media.retain(|item| item.url != payload.url);
        guard.media.insert(0, payload.clone());
        guard.media.truncate(MAX_MEDIA_ITEMS);
    }
    window
        .app_handle()
        .emit("sniffer:found", payload)
        .map_err(|error| error.to_string())
}

/// 归一化 URL 用于「是否同一页面」判断：忽略片段、末尾斜杠和端口
fn normalize_url(raw: &str) -> String {
    match Url::parse(raw.trim()) {
        Ok(url) => {
            let trimmed = url.path().trim_end_matches('/');
            let path = if trimmed.is_empty() { "/" } else { trimmed };
            let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
            format!(
                "{}://{}{}{}",
                url.scheme(),
                url.host_str().unwrap_or_default(),
                path,
                query
            )
        }
        Err(_) => raw.trim().trim_end_matches('/').to_string(),
    }
}

/// 侧栏拉取当前页面的媒体列表（不同页面的历史记录不再混在一起）
pub fn list(
    state: &Mutex<SnifferState>,
    page_url: Option<&str>,
) -> Result<Vec<SnifferPayload>, String> {
    let guard = state.lock().map_err(|error| error.to_string())?;
    let current = page_url.map(normalize_url);
    Ok(guard
        .media
        .iter()
        .filter(|item| match &current {
            Some(current) => normalize_url(&item.page_url) == *current,
            None => true,
        })
        .cloned()
        .collect())
}

/// 侧栏「解析当前页面」：把浏览器当前地址交给主窗口解析
pub fn enqueue_page(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    let current = window.url().map_err(|error| error.to_string())?;
    if current.scheme() != "http" && current.scheme() != "https" {
        return Err("当前页面不是网页，无法解析。".into());
    }
    let payload = SnifferPayload {
        id: Uuid::new_v4().to_string(),
        url: current.to_string(),
        media_type: "page",
        title: String::new(),
        page_url: current.to_string(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default(),
    };
    app.emit("sniffer:enqueue", payload)
        .map_err(|error| error.to_string())
}

/// 侧栏点「添加」：只接受已捕获列表中的地址，防止第三方页面伪造下载
pub fn enqueue(app: &AppHandle, state: &Mutex<SnifferState>, url: &str) -> Result<(), String> {
    let media_url = parse_http_url(url)?;
    let payload = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        guard
            .media
            .iter()
            .find(|item| item.url == media_url.as_str())
            .cloned()
    }
    .ok_or_else(|| "该地址不在已捕获列表中，请先在页面里播放视频。".to_string())?;
    app.emit("sniffer:enqueue", payload)
        .map_err(|error| error.to_string())
}

/// 拼一行 Netscape Cookie 记录（HttpOnly 用 `#HttpOnly_` 前缀，yt-dlp 可识别）
#[allow(clippy::too_many_arguments)]
fn netscape_cookie_line(
    domain: &str,
    http_only: bool,
    include_sub: &str,
    path: &str,
    secure: &str,
    expires: i64,
    name: &str,
    value: &str,
) -> String {
    let prefix = if http_only {
        format!("#HttpOnly_{domain}")
    } else {
        domain.to_string()
    };
    format!("{prefix}\t{include_sub}\t{path}\t{secure}\t{expires}\t{name}\t{value}\n")
}

/// 把内置浏览器当前站点的 Cookie 导出为 Netscape 格式，按域名保存，供 yt-dlp 使用。
/// 域名取自 webview 当前页面，不接受调用方传入的 URL。
pub async fn export_cookies(app: &AppHandle) -> Result<crate::ytdlp::SiteLogin, String> {
    let window = browser_window(app)?;
    let current = window.url().map_err(|error| error.to_string())?;
    let host = current
        .host_str()
        .ok_or_else(|| "当前页面没有有效域名".to_string())?
        .to_ascii_lowercase();
    let domain = crate::ytdlp::registrable_domain(&host);
    // 注意：这里用全量 cookies 自行匹配父域，cookies_for_url 在 macOS 只做精确匹配
    let cookies = window.cookies().map_err(|error| error.to_string())?;

    let mut text = String::from("# Netscape HTTP Cookie File\n# Generated by Vidgo\n\n");
    let mut count = 0usize;
    for cookie in cookies {
        let Some(cookie_domain) = cookie.domain() else { continue };
        let bare = cookie_domain.trim_start_matches('.').to_ascii_lowercase();
        if bare.is_empty() || (host != bare && !host.ends_with(&format!(".{bare}"))) {
            continue;
        }
        let name = cookie.name();
        let value = cookie.value();
        let invalid = |s: &str| s.chars().any(|c| c == '\t' || c == '\n' || c == '\r');
        if name.is_empty() || invalid(name) || invalid(value) {
            continue;
        }
        let include_sub = if cookie_domain.starts_with('.') { "TRUE" } else { "FALSE" };
        let path = cookie.path().unwrap_or("/");
        let secure = if cookie.secure().unwrap_or(false) { "TRUE" } else { "FALSE" };
        let expires = cookie
            .expires_datetime()
            .map(|time| time.unix_timestamp())
            .unwrap_or(0)
            .max(0);
        text.push_str(&netscape_cookie_line(
            cookie_domain,
            cookie.http_only().unwrap_or(false),
            include_sub,
            path,
            secure,
            expires,
            name,
            value,
        ));
        count += 1;
    }
    if count == 0 {
        return Err("当前站点没有可导出的 Cookie，请先完成登录。".into());
    }

    crate::ytdlp::save_site_cookies(&domain, &text)?;
    Ok(crate::ytdlp::SiteLogin {
        domain,
        cookie_count: u32::try_from(count).unwrap_or(u32::MAX),
        saved_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or_default(),
        size: u64::try_from(text.len()).unwrap_or_default(),
    })
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
    fn scopes_media_list_to_current_page() {
        let item = |url: &str, page: &str| SnifferPayload {
            id: url.to_string(),
            url: url.to_string(),
            media_type: "mp4",
            title: "t".into(),
            page_url: page.into(),
            timestamp: 0,
        };
        let state = Mutex::new(SnifferState {
            media: vec![
                item(
                    "https://cdn.example.com/a.mp4",
                    "https://www.bilibili.com/video/BV1",
                ),
                item(
                    "https://cdn.example.com/b.mp4",
                    "https://www.douyin.com/video/2",
                ),
            ],
            ..Default::default()
        });
        let bili = list(&state, Some("https://www.bilibili.com/video/BV1/")).unwrap();
        assert_eq!(bili.len(), 1);
        assert!(bili[0].url.ends_with("a.mp4"));
        assert!(list(&state, Some("https://www.bilibili.com/video/BV9"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn netscape_cookie_line_marks_httponly_and_flags() {
        let line = netscape_cookie_line(
            ".bilibili.com",
            true,
            "TRUE",
            "/",
            "TRUE",
            1_700_000_000,
            "SESSDATA",
            "abc123",
        );
        assert_eq!(
            line,
            "#HttpOnly_.bilibili.com\tTRUE\t/\tTRUE\t1700000000\tSESSDATA\tabc123\n"
        );
        let plain = netscape_cookie_line("example.com", false, "FALSE", "/", "FALSE", 0, "a", "b");
        assert_eq!(plain, "example.com\tFALSE\t/\tFALSE\t0\ta\tb\n");
    }

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
