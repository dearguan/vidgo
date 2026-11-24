use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub id: String,
    pub url: String,
    pub title: String,
    pub created_at: i64,
    #[serde(default)]
    pub last_checked_at: Option<i64>,
    #[serde(default)]
    pub entries: Vec<SubscriptionEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionEntry {
    pub entry_id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub upload_date: Option<String>,
    pub first_seen_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionCheckResult {
    pub subscription: Subscription,
    pub new_entry_ids: Vec<String>,
}

pub fn validate_http_url(url: &str) -> Result<String, String> {
    crate::ytdlp::sanitize_url(url)
        .ok_or_else(|| "订阅地址无效：请粘贴完整的 http/https 链接。".to_string())
}

pub fn add(
    subscriptions: &mut Vec<Subscription>,
    url: &str,
    now_ms: i64,
) -> Result<Subscription, String> {
    let url = validate_http_url(url)?;
    let url = crate::ytdlp::canonicalize_url(&url);
    if crate::ytdlp::is_douyin_user_page(&url) {
        return Err("抖音个人主页暂不支持订阅（解析引擎不支持）。请改用具体视频或合集链接。".into());
    }
    if subscriptions.iter().any(|item| item.url == url) {
        return Err("该订阅已存在。".into());
    }
    let subscription = Subscription {
        id: Uuid::new_v4().to_string(),
        title: url.clone(),
        url,
        created_at: now_ms,
        last_checked_at: None,
        entries: Vec::new(),
    };
    subscriptions.push(subscription.clone());
    Ok(subscription)
}

pub fn merge_entries(current: &Subscription, fetched: Subscription) -> SubscriptionCheckResult {
    let first_seen: HashMap<&str, i64> = current
        .entries
        .iter()
        .map(|entry| (entry.entry_id.as_str(), entry.first_seen_at))
        .collect();
    let mut seen = HashSet::new();
    let mut new_entry_ids = Vec::new();
    let mut entries = Vec::new();

    for mut entry in fetched.entries {
        if !seen.insert(entry.entry_id.clone()) {
            continue;
        }
        if let Some(first_seen_at) = first_seen.get(entry.entry_id.as_str()) {
            entry.first_seen_at = *first_seen_at;
        } else {
            new_entry_ids.push(entry.entry_id.clone());
        }
        entries.push(entry);
    }
    for entry in &current.entries {
        if seen.insert(entry.entry_id.clone()) {
            entries.push(entry.clone());
        }
    }
    entries.truncate(100);
    new_entry_ids.retain(|id| entries.iter().any(|entry| &entry.entry_id == id));

    let title = if fetched.title.trim().is_empty() {
        current.title.clone()
    } else {
        fetched.title
    };
    SubscriptionCheckResult {
        subscription: Subscription {
            id: current.id.clone(),
            url: current.url.clone(),
            title,
            created_at: current.created_at,
            last_checked_at: fetched.last_checked_at.or(current.last_checked_at),
            entries,
        },
        new_entry_ids,
    }
}

pub fn parse_flat_playlist(
    json: &str,
    original_url: &str,
    now_ms: i64,
) -> Result<Subscription, String> {
    let original_url = validate_http_url(original_url)?;
    let root: Value = serde_json::from_str(json).map_err(|e| format!("订阅解析结果异常：{e}"))?;
    let root = root
        .as_object()
        .ok_or_else(|| "订阅解析结果不是 JSON 对象。".to_string())?;
    let title = ["title", "playlist_title", "uploader", "channel"]
        .iter()
        .find_map(|key| value_string(root.get(*key)))
        .unwrap_or_else(|| original_url.clone());
    let raw_entries = match root.get("entries") {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(entries)) => entries.as_slice(),
        Some(_) => return Err("订阅解析结果中的 entries 不是数组。".into()),
    };
    let entries = raw_entries
        .iter()
        .filter_map(|value| parse_entry(value, now_ms))
        .collect();

    Ok(Subscription {
        id: Uuid::new_v4().to_string(),
        url: original_url,
        title,
        created_at: now_ms,
        last_checked_at: Some(now_ms),
        entries,
    })
}

pub fn fetch_flat_playlist(
    url: &str,
    settings: &crate::store::Settings,
) -> Result<Subscription, String> {
    let url = validate_http_url(url)?;
    if crate::ytdlp::is_douyin_user_page(&url) {
        return Err("抖音个人主页暂不支持订阅（解析引擎不支持）。请改用具体视频或合集链接。".into());
    }
    let url = crate::ytdlp::canonicalize_url(&url);
    let proxy = crate::ytdlp::effective_proxy(settings, &url);

    let mut attempt = 0usize;
    loop {
        let cookies = if attempt == 0 {
            crate::ytdlp::cookie_args(settings, &url)
        } else {
            crate::ytdlp::cookie_args_merged(settings, &url)
        };
        let mut command = Command::new(crate::ytdlp::ytdlp_path());
        crate::ytdlp::hide_console(&mut command);
        command.args([
            "-J",
            "--flat-playlist",
            "--playlist-end",
            "100",
            "--no-warnings",
        ]);
        if !proxy.is_empty() {
            command.arg("--proxy").arg(&proxy);
        }
        command.args(&cookies.args);
        let output = command
            .arg(&url)
            .output()
            .map_err(|e| format!("启动 yt-dlp 检查订阅失败：{e}"))?;
        if output.status.success() {
            let stdout = String::from_utf8(output.stdout)
                .map_err(|e| format!("yt-dlp 订阅输出不是有效 UTF-8：{e}"))?;
            return parse_flat_playlist(&stdout, &url, now_ms());
        }

        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let can_merge = attempt == 0
            && crate::ytdlp::has_site_cookie_file(&url)
            && crate::ytdlp::has_browser_cookie_source(settings)
            && [
                "Fresh cookies",
                "Sign in to confirm",
                "login required",
                "HTTP Error 403",
            ]
            .iter()
            .any(|key| detail.to_lowercase().contains(&key.to_lowercase()));
        if can_merge {
            attempt = 1;
            continue;
        }
        return Err(if detail.is_empty() {
            format!("yt-dlp 检查订阅失败，退出状态：{}", output.status)
        } else {
            format!("yt-dlp 检查订阅失败：{detail}")
        });
    }
}

fn parse_entry(value: &Value, now_ms: i64) -> Option<SubscriptionEntry> {
    let entry = value.as_object()?;
    let url = ["webpage_url", "original_url", "url"]
        .iter()
        .find_map(|key| value_string(entry.get(*key)))?;
    let url = validate_http_url(&url).ok()?;
    let entry_id = value_string(entry.get("id")).unwrap_or_else(|| url.clone());
    let title = value_string(entry.get("title")).unwrap_or_else(|| entry_id.clone());
    let thumbnail = value_string(entry.get("thumbnail")).or_else(|| {
        entry
            .get("thumbnails")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .rev()
            .find_map(|thumbnail| value_string(thumbnail.get("url")))
    });
    let duration = entry
        .get("duration")
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .filter(|value| value.is_finite() && *value >= 0.0);

    Some(SubscriptionEntry {
        entry_id,
        url,
        title,
        thumbnail,
        duration,
        upload_date: value_string(entry.get("upload_date")),
        first_seen_at: now_ms,
    })
}

fn value_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn now_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_playlist_fixture_and_url_priority() {
        let fixture = r#"{
            "id": "channel-1",
            "title": "示例频道",
            "entries": [
                {
                    "id": "one",
                    "webpage_url": "https://example.com/watch/one",
                    "original_url": "https://example.com/original/one",
                    "url": "https://example.com/raw/one",
                    "title": "第一条",
                    "thumbnail": "https://example.com/one.jpg",
                    "duration": 12.5,
                    "upload_date": "20261122"
                },
                {
                    "id": "two",
                    "original_url": "https://example.com/original/two",
                    "url": "https://example.com/raw/two",
                    "title": "第二条",
                    "thumbnails": [{"url": "https://example.com/small.jpg"}, {"url": "https://example.com/large.jpg"}]
                },
                {
                    "id": "three",
                    "url": "https://example.com/raw/three",
                    "title": "第三条"
                }
            ]
        }"#;
        let subscription =
            parse_flat_playlist(fixture, "https://example.com/channel", 1234).unwrap();
        assert_eq!(subscription.title, "示例频道");
        assert_eq!(subscription.created_at, 1234);
        assert_eq!(subscription.last_checked_at, Some(1234));
        assert_eq!(subscription.entries.len(), 3);
        assert_eq!(subscription.entries[0].entry_id, "one");
        assert_eq!(subscription.entries[0].url, "https://example.com/watch/one");
        assert_eq!(subscription.entries[0].duration, Some(12.5));
        assert_eq!(
            subscription.entries[0].upload_date.as_deref(),
            Some("20261122")
        );
        assert_eq!(
            subscription.entries[1].url,
            "https://example.com/original/two"
        );
        assert_eq!(
            subscription.entries[1].thumbnail.as_deref(),
            Some("https://example.com/large.jpg")
        );
        assert_eq!(subscription.entries[2].url, "https://example.com/raw/three");
        assert!(subscription
            .entries
            .iter()
            .all(|entry| entry.first_seen_at == 1234));
    }

    #[test]
    fn parsing_tolerates_missing_optional_fields_and_skips_entries_without_url() {
        let fixture = r#"{
            "uploader": "上传者",
            "entries": [
                {"id": "missing-url", "title": "跳过"},
                {"url": "https://example.com/video", "duration": "9.5"},
                {"id": 123, "url": "https://example.com/numeric", "thumbnail": null},
                null
            ]
        }"#;
        let subscription = parse_flat_playlist(fixture, "https://example.com/list", 99).unwrap();
        assert_eq!(subscription.title, "上传者");
        assert_eq!(subscription.entries.len(), 2);
        assert_eq!(
            subscription.entries[0].entry_id,
            "https://example.com/video"
        );
        assert_eq!(subscription.entries[0].title, "https://example.com/video");
        assert_eq!(subscription.entries[0].duration, Some(9.5));
        assert_eq!(subscription.entries[0].thumbnail, None);
        assert_eq!(subscription.entries[0].upload_date, None);
        assert_eq!(subscription.entries[1].entry_id, "123");
        assert!(parse_flat_playlist("not json", "https://example.com/list", 99).is_err());
    }

    #[test]
    fn add_validates_url_and_rejects_duplicates() {
        let mut subscriptions = Vec::new();
        let subscription = add(&mut subscriptions, " https://example.com/channel ", 10).unwrap();
        assert_eq!(subscription.url, "https://example.com/channel");
        assert_eq!(subscription.created_at, 10);
        assert_eq!(subscription.last_checked_at, None);
        assert!(add(&mut subscriptions, "https://example.com/channel", 20).is_err());
        assert!(add(&mut subscriptions, "file:///tmp/channel", 20).is_err());
        assert_eq!(subscriptions.len(), 1);
    }

    #[test]
    fn merge_marks_new_entries_preserves_first_seen_and_keeps_old_on_failure() {
        let current = Subscription {
            id: "subscription-id".into(),
            url: "https://example.com/channel".into(),
            title: "旧标题".into(),
            created_at: 10,
            last_checked_at: Some(20),
            entries: vec![
                SubscriptionEntry {
                    entry_id: "old".into(),
                    url: "https://example.com/old".into(),
                    title: "旧条目".into(),
                    thumbnail: None,
                    duration: None,
                    upload_date: None,
                    first_seen_at: 15,
                },
                SubscriptionEntry {
                    entry_id: "cached".into(),
                    url: "https://example.com/cached".into(),
                    title: "缓存条目".into(),
                    thumbnail: None,
                    duration: None,
                    upload_date: None,
                    first_seen_at: 16,
                },
            ],
        };
        let fetched = Subscription {
            id: "temporary-id".into(),
            url: current.url.clone(),
            title: "新标题".into(),
            created_at: 30,
            last_checked_at: Some(30),
            entries: vec![
                SubscriptionEntry {
                    entry_id: "old".into(),
                    url: "https://example.com/old-updated".into(),
                    title: "更新条目".into(),
                    thumbnail: None,
                    duration: Some(1.0),
                    upload_date: None,
                    first_seen_at: 30,
                },
                SubscriptionEntry {
                    entry_id: "new".into(),
                    url: "https://example.com/new".into(),
                    title: "新增条目".into(),
                    thumbnail: None,
                    duration: None,
                    upload_date: None,
                    first_seen_at: 30,
                },
            ],
        };

        let result = merge_entries(&current, fetched);
        assert_eq!(result.new_entry_ids, vec!["new"]);
        assert_eq!(result.subscription.id, "subscription-id");
        assert_eq!(result.subscription.created_at, 10);
        assert_eq!(result.subscription.last_checked_at, Some(30));
        assert_eq!(result.subscription.title, "新标题");
        assert_eq!(result.subscription.entries.len(), 3);
        assert_eq!(result.subscription.entries[0].first_seen_at, 15);
        assert_eq!(result.subscription.entries[0].title, "更新条目");
        assert_eq!(result.subscription.entries[2].entry_id, "cached");
        assert_eq!(current.entries[0].title, "旧条目");

        let failure: Result<Subscription, String> = Err("网络失败".into());
        let failed_result = failure.map(|fetched| merge_entries(&current, fetched));
        assert!(failed_result.is_err());
        assert_eq!(current.entries.len(), 2);
        assert_eq!(current.last_checked_at, Some(20));
    }
}
