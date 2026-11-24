use crate::net;
use crate::store::{AppStore, TaskStatus};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;
use uuid::Uuid;

const REPORT_LABEL: &str = "app-report";
const MAX_DETAIL_CHARS: usize = 20_000;
const API_BASE: &str = "https://api.github.com";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInput {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub js_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportResult {
    pub report_id: String,
    /// github：已创建 Issue；local：只保存在本机
    pub mode: String,
    pub issue_url: Option<String>,
    pub issue_number: Option<u64>,
    pub saved_path: String,
    pub upload_error: Option<String>,
    pub text: String,
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "parse" => "解析失败",
        "download" => "下载失败",
        _ => "其他问题",
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let kept: String = text.chars().take(max).collect();
        format!("{kept}\n…（内容已截断）")
    }
}

fn one_line(text: &str, max: usize) -> String {
    truncate(text.replace(['\r', '\n'], " ").trim(), max)
}

fn reports_dir(store: &AppStore) -> PathBuf {
    store
        .path
        .parent()
        .map(|p| p.join("reports"))
        .unwrap_or_else(|| PathBuf::from("reports"))
}

#[allow(clippy::too_many_arguments)]
fn render_body(
    report_id: &str,
    app_version: &str,
    input: &ReportInput,
    proxy: &str,
    cookie_browser: &str,
    ytdlp: &str,
    recent_failures: &[String],
    site_logins: &[crate::ytdlp::SiteLogin],
    created_at: i64,
) -> String {
    let mut body = String::new();
    body.push_str("### 应用内上报\n\n");
    body.push_str(&format!("- **上报编号**：`{report_id}`\n"));
    body.push_str(&format!("- **应用版本**：{app_version}\n"));
    body.push_str(&format!(
        "- **运行平台**：{} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    body.push_str(&format!("- **问题类型**：{}\n", kind_label(&input.kind)));
    body.push_str(&format!("- **时间戳**：{created_at}\n"));
    body.push_str(&format!("- **代理**：{}\n", net::mask_proxy(proxy)));
    body.push_str(&format!(
        "- **Cookie 来源**：{}\n",
        if cookie_browser.trim().is_empty() {
            "未设置"
        } else {
            cookie_browser
        }
    ));
    body.push_str(&format!(
        "- **yt-dlp**：{}\n",
        if ytdlp.trim().is_empty() {
            "未知"
        } else {
            ytdlp
        }
    ));
    if site_logins.is_empty() {
        body.push_str("- **站点登录**：无\n\n");
    } else {
        let items = site_logins
            .iter()
            .map(|login| format!("{}（{} 个 Cookie）", login.domain, login.cookie_count))
            .collect::<Vec<_>>()
            .join("、");
        body.push_str(&format!("- **站点登录**：{items}\n\n"));
    }

    body.push_str("#### 标题\n\n");
    body.push_str(&format!("{}\n\n", one_line(&input.title, 300)));

    if !input.url.trim().is_empty() {
        body.push_str("#### 链接\n\n");
        body.push_str(&format!("{}\n\n", input.url.trim()));
    }

    if !input.message.trim().is_empty() {
        body.push_str("#### 错误信息\n\n");
        body.push_str(&format!("{}\n\n", input.message.trim()));
    }

    if !input.detail.trim().is_empty() {
        body.push_str("#### 技术详情\n\n```text\n");
        body.push_str(&truncate(input.detail.trim(), MAX_DETAIL_CHARS));
        body.push_str("\n```\n\n");
    }

    if !recent_failures.is_empty() {
        body.push_str("#### 最近的失败任务\n\n");
        for item in recent_failures {
            body.push_str(&format!("- {item}\n"));
        }
        body.push('\n');
    }

    if !input.js_errors.is_empty() {
        body.push_str("#### 前端异常\n\n```text\n");
        let joined = input.js_errors.join("\n");
        body.push_str(&truncate(&joined, 4_000));
        body.push_str("\n```\n\n");
    }

    if !input.note.trim().is_empty() {
        body.push_str("#### 用户备注\n\n");
        body.push_str(&truncate(input.note.trim(), 4_000));
        body.push('\n');
    }

    body.push_str("\n---\n\n> 由 Vidgo 内置上报功能自动生成。\n");
    body
}

async fn post_issue(
    repo: &str,
    token: &str,
    proxy: &str,
    title: &str,
    body: &str,
) -> Result<(String, u64), String> {
    let client = net::client(proxy, Duration::from_secs(25))?;
    let auth = format!("Bearer {token}");
    let label_url = format!("{API_BASE}/repos/{repo}/labels");
    // 标签不存在时创建，失败不影响后续提交
    let _ = client
        .post(&label_url)
        .header("Authorization", &auth)
        .header("Accept", "application/vnd.github+json")
        .json(&serde_json::json!({
            "name": REPORT_LABEL,
            "color": "d73a4a",
            "description": "由 Vidgo 应用内上报自动创建",
        }))
        .send()
        .await;

    let response = client
        .post(format!("{API_BASE}/repos/{repo}/issues"))
        .header("Authorization", &auth)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&serde_json::json!({
            "title": one_line(title, 200),
            "body": body,
            "labels": [REPORT_LABEL],
        }))
        .send()
        .await
        .map_err(|e| format!("连接 GitHub 失败：{e}"))?;

    let status = response.status();
    let raw = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let hint = match status.as_u16() {
            401 => "Token 无效或已过期。",
            403 => "Token 缺少 Issues 读写权限，或触发了接口频率限制。",
            404 => "仓库不存在，或 Token 无权访问该仓库。",
            410 => "该仓库的 Issues 功能未开启。",
            422 => "Issue 内容被 GitHub 拒绝（可能是标签或标题不合法）。",
            _ => "GitHub 返回了错误。",
        };
        return Err(format!("{hint}（HTTP {}）{}", status.as_u16(), one_line(&raw, 200)));
    }

    #[derive(Deserialize)]
    struct Created {
        number: u64,
        html_url: String,
    }
    let created: Created =
        serde_json::from_str(&raw).map_err(|e| format!("解析 GitHub 返回结果失败：{e}"))?;
    Ok((created.html_url, created.number))
}

pub async fn submit(
    app: &AppHandle,
    store: &Mutex<AppStore>,
    input: ReportInput,
) -> Result<ReportResult, String> {
    let version = app.package_info().version.to_string();
    submit_with_version(&version, store, input).await
}

pub(crate) async fn submit_with_version(
    app_version: &str,
    store: &Mutex<AppStore>,
    input: ReportInput,
) -> Result<ReportResult, String> {
    let (settings, recent_failures, dir, site_logins) = {
        let s = store.lock().map_err(|e| e.to_string())?;
        let failures = s
            .data
            .tasks
            .iter()
            .rev()
            .filter(|t| matches!(t.status, TaskStatus::Failed | TaskStatus::Cancelled))
            .take(3)
            .map(|t| {
                format!(
                    "{} | {} | {}",
                    one_line(&t.title, 80),
                    t.url,
                    one_line(t.error.as_deref().unwrap_or("未知错误"), 160)
                )
            })
            .collect::<Vec<_>>();
        (
            s.data.settings.clone(),
            failures,
            reports_dir(&s),
            crate::ytdlp::list_site_logins(),
        )
    };

    let report_id = format!("VID-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let app_version = app_version.to_string();
    let ytdlp = crate::commands::ytdlp_version();

    let body = render_body(
        &report_id,
        &app_version,
        &input,
        &settings.proxy,
        &settings.cookies_from_browser,
        &ytdlp,
        &recent_failures,
        &site_logins,
        created_at,
    );

    std::fs::create_dir_all(&dir).map_err(|e| format!("创建报告目录失败：{e}"))?;
    let saved_path = dir.join(format!("{report_id}.json"));
    let stored = serde_json::json!({
        "reportId": report_id,
        "createdAt": created_at,
        "appVersion": app_version,
        "platform": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "input": &input,
        "body": body,
    });
    let pretty = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(&saved_path, pretty).map_err(|e| format!("保存报告失败：{e}"))?;

    let mut result = ReportResult {
        report_id: report_id.clone(),
        mode: "local".into(),
        issue_url: None,
        issue_number: None,
        saved_path: saved_path.to_string_lossy().to_string(),
        upload_error: None,
        text: body.clone(),
    };

    let token = settings.github_token.trim();
    if token.is_empty() {
        return Ok(result);
    }
    let repo = if settings.report_repo.trim().is_empty() {
        "dearguan/vidgo"
    } else {
        settings.report_repo.trim()
    };
    let title = format!("[{}] {}", kind_label(&input.kind), input.title);
    match post_issue(repo, token, &settings.proxy, &title, &body).await {
        Ok((url, number)) => {
            result.mode = "github".into();
            result.issue_url = Some(url);
            result.issue_number = Some(number);
        }
        Err(e) => result.upload_error = Some(e),
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_proxy_credentials() {
        assert_eq!(net::mask_proxy(""), "未设置");
        assert_eq!(net::mask_proxy("http://127.0.0.1:7890"), "http://127.0.0.1:7890");
        assert_eq!(
            net::mask_proxy("socks5://user:pass@127.0.0.1:1080"),
            "socks5://127.0.0.1:1080"
        );
    }

    #[test]
    fn report_body_contains_key_facts() {
        let input = ReportInput {
            kind: "parse".into(),
            title: "测试视频".into(),
            url: "https://example.com/video".into(),
            message: "解析失败".into(),
            detail: "ERROR: unsupported url".into(),
            note: "第一次遇到".into(),
            ..Default::default()
        };
        let body = render_body(
            "VID-1234",
            "0.4.4",
            &input,
            "http://127.0.0.1:7890",
            "auto",
            "2025.11.01",
            &["旧任务 | https://example.com/a | 网络错误".into()],
            &[],
            1763,
        );
        assert!(body.contains("VID-1234"));
        assert!(body.contains("0.4.4"));
        assert!(body.contains("https://example.com/video"));
        assert!(body.contains("ERROR: unsupported url"));
        assert!(body.contains("第一次遇到"));
        assert!(body.contains("旧任务"));
    }

    #[tokio::test]
    async fn saves_report_locally_without_token() {
        use crate::store::{AppStore, Settings, StoreData};

        let dir = std::env::temp_dir().join(format!("vidgo-report-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Mutex::new(AppStore {
            data: StoreData {
                settings: Settings::default(),
                tasks: vec![],
                subscriptions: vec![],
            },
            path: dir.join("videogo.json"),
        });
        let result = submit_with_version(
            "0.4.4",
            &store,
            ReportInput {
                kind: "parse".into(),
                title: "测试解析失败".into(),
                url: "https://example.com/v".into(),
                message: "无法解析".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(result.mode, "local");
        assert!(result.report_id.starts_with("VID-"));
        assert!(result.upload_error.is_none());
        let saved = std::fs::read_to_string(&result.saved_path).unwrap();
        assert!(saved.contains("0.4.4"));
        assert!(saved.contains("https://example.com/v"));
        assert!(saved.contains("测试解析失败"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
