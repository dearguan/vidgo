import { useEffect, useState, type ReactElement, type ReactNode } from 'react'
import type { Settings, ThemeMode } from '../../../shared/types'
import { Icon } from '../Icon'
import { useApp, describeUpdateProgress } from '../state'
import { api } from '../api'
import { formatTime } from '../utils'

type CookieBrowser = Settings['cookieBrowser']

/** 常见站点显示名（没有命中时直接显示域名） */
const DOMAIN_LABELS: Record<string, string> = {
  'douyin.com': '抖音',
  'bilibili.com': '哔哩哔哩',
  'youtube.com': 'YouTube',
  'xiaohongshu.com': '小红书',
  'kuaishou.com': '快手',
  'weibo.com': '微博',
  'qq.com': '腾讯视频',
  'iqiyi.com': '爱奇艺',
  'youku.com': '优酷',
  'tiktok.com': 'TikTok',
  'x.com': 'X',
  'instagram.com': 'Instagram'
}

const BROWSER_OPTIONS: Array<{ value: CookieBrowser; label: string; hint?: string }> = [
  { value: 'none', label: '不读取（推荐，使用站点登录）' },
  { value: 'chrome', label: 'Google Chrome' },
  { value: 'edge', label: 'Microsoft Edge' },
  { value: 'brave', label: 'Brave' },
  { value: 'firefox', label: 'Firefox' },
  { value: 'safari', label: 'Safari' },
  { value: 'vivaldi', label: 'Vivaldi' },
  { value: 'opera', label: 'Opera' }
]

function Group({ title, onReset, children }: { title: string; onReset?: () => void; children: ReactNode }): ReactElement {
  return (
    <section className="card settings-group">
      <div className="settings-group-head">
        <h3>{title}</h3>
        {onReset && (
          <button className="btn-text muted" onClick={onReset}>
            恢复默认
          </button>
        )}
      </div>
      {children}
    </section>
  )
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }): ReactElement {
  return (
    <div className="settings-row">
      <div className="settings-label">
        <span>{label}</span>
        {hint && <p>{hint}</p>}
      </div>
      <div className="settings-control">{children}</div>
    </div>
  )
}

export function SettingsPage(): ReactElement {
  const {
    settings,
    appInfo,
    updateSettings,
    chooseDir,
    openBuiltinBrowser,
    siteLogins,
    removeSiteLogin,
    notify,
    updateInfo,
    checkingUpdate,
    installingUpdate,
    updateProgress,
    updateError,
    checkUpdate,
    installUpdate
  } = useApp()
  const [proxy, setProxy] = useState('')
  const [token, setToken] = useState('')
  const [repo, setRepo] = useState('')
  const [loginUrl, setLoginUrl] = useState('')

  const openLogin = (target?: string): void => {
    const raw = target?.trim()
    const url = raw ? (raw.startsWith('http://') || raw.startsWith('https://') ? raw : `https://${raw}`) : undefined
    void openBuiltinBrowser(url).catch((e) => notify(e instanceof Error ? e.message : String(e), 'err'))
  }

  useEffect(() => {
    setProxy(settings?.proxy ?? '')
    setToken(settings?.githubToken ?? '')
    setRepo(settings?.reportRepo ?? '')
  }, [settings?.proxy, settings?.githubToken, settings?.reportRepo])

  if (!settings) return <div className="page" />

  const patch = (p: Partial<Settings>): void => {
    void updateSettings(p)
  }

  return (
    <div className="page settings-page">
      <Group title="常规" onReset={() => patch({ theme: 'system', clipboardWatch: true })}>
        <Row label="默认保存路径" hint="下载的视频和音频保存到这个文件夹">
          <div className="path-control">
            <span className="save-path" title={settings.saveDir}>
              {settings.saveDir}
            </span>
            <button className="btn secondary small" onClick={() => void chooseDir()}>
              更改
            </button>
          </div>
        </Row>
        <Row label="主题模式" hint="切换后立即生效，无需重启">
          <div className="segment-group compact" role="radiogroup" aria-label="主题模式">
            {(
              [
                ['light', '浅色'],
                ['dark', '深色'],
                ['system', '跟随系统']
              ] as Array<[ThemeMode, string]>
            ).map(([value, label]) => (
              <button
                key={value}
                role="radio"
                aria-checked={settings.theme === value}
                className={`segment ${settings.theme === value ? 'selected' : ''}`}
                onClick={() => patch({ theme: value })}
              >
                {label}
              </button>
            ))}
          </div>
        </Row>
        <Row label="剪贴板链接检测" hint="窗口激活时检测剪贴板中的视频链接，仅提示，不自动解析">
          <button
            role="switch"
            aria-checked={settings.clipboardWatch}
            aria-label="剪贴板链接检测"
            className={`switch ${settings.clipboardWatch ? 'on' : ''}`}
            onClick={() => patch({ clipboardWatch: !settings.clipboardWatch })}
          >
            <span className="switch-dot" />
          </button>
        </Row>
      </Group>

      <Group title="下载" onReset={() => patch({ concurrency: 3 })}>
        <Row label="同时下载任务数" hint="范围 1–6，超出任务进入排队">
          <input
            className="input number"
            type="number"
            min={1}
            max={6}
            value={settings.concurrency}
            aria-label="同时下载任务数"
            onChange={(e) => {
              const v = Number(e.target.value)
              if (v >= 1 && v <= 6) patch({ concurrency: v })
            }}
          />
        </Row>
      </Group>

      <Group title={`站点登录${siteLogins.length > 0 ? `（${siteLogins.length}）` : ''}`}>
        <Row
          label="登录新站点"
          hint="填入网站地址，在打开的内置浏览器里登录。登录后点页面右上角「完成登录」，Cookie 会按站点保存，之后解析和下载该站点会自动使用。"
        >
          <div className="settings-inline site-login-add">
            <input
              className="input"
              value={loginUrl}
              placeholder="https://www.bilibili.com"
              aria-label="要登录的网站地址"
              onChange={(e) => setLoginUrl(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && loginUrl.trim()) openLogin(loginUrl.trim())
              }}
            />
            <button className="btn primary small" disabled={!loginUrl.trim()} onClick={() => openLogin(loginUrl.trim())}>
              打开并登录
            </button>
          </div>
        </Row>

        {siteLogins.length === 0 ? (
          <p className="site-login-empty">
            还没有保存任何站点登录。登录一次后，该站点的下载会一直带着 Cookie，不需要重复配置。
          </p>
        ) : (
          <ul className="site-login-list">
            {siteLogins.map((item) => (
              <li className="site-login-item" key={item.domain}>
                <div className="site-login-main">
                  <span className="site-login-domain">
                    {DOMAIN_LABELS[item.domain] ?? item.domain}
                    {DOMAIN_LABELS[item.domain] && <span className="site-login-host">{item.domain}</span>}
                  </span>
                  <span className="site-login-meta">
                    {item.cookieCount} 个 Cookie · 保存于 {formatTime(item.savedAt)}
                  </span>
                </div>
                <div className="site-login-actions">
                  <button className="btn secondary small" onClick={() => openLogin(item.domain)}>
                    重新登录
                  </button>
                  <button className="btn-text muted" onClick={() => void removeSiteLogin(item.domain)}>
                    删除
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}

        <Row
          label="读取系统浏览器 Cookie"
          hint="新版 Chrome / Edge 常因加密保护读取失败。建议优先用上面的内置浏览器登录；此选项仅在解析失败时兜底。"
        >
          <select
            className="input select"
            value={settings.cookieBrowser === 'auto' ? 'none' : settings.cookieBrowser || 'none'}
            aria-label="读取系统浏览器 Cookie"
            onChange={(e) => patch({ cookieBrowser: e.target.value as CookieBrowser })}
          >
            {BROWSER_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </Row>
      </Group>

      <Group title="网络与代理">
        <Row label="代理服务器" hint="支持 HTTP / SOCKS5，例如 http://127.0.0.1:7890。留空使用系统网络，输入框失去焦点后生效">
          <input
            className="input"
            value={proxy}
            placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
            aria-label="代理服务器"
            onChange={(e) => setProxy(e.target.value)}
            onBlur={() => {
              if (proxy !== settings.proxy) patch({ proxy: proxy.trim() })
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') e.currentTarget.blur()
            }}
          />
        </Row>
      </Group>

      <Group title="版本与更新">
        <Row label="软件版本" hint="发现新版本后可在此直接安装，安装完成后会自动重启">
          <div className="settings-inline">
            <span className="muted-text">v{appInfo?.version ?? '…'}</span>
            <button
              className="btn secondary small"
              disabled={checkingUpdate || installingUpdate}
              onClick={() => void checkUpdate()}
            >
              {checkingUpdate ? '检查中…' : '检查更新'}
            </button>
          </div>
        </Row>
        {updateInfo?.available ? (
          <div className="update-card">
            <div className="update-card-text">
              <strong>发现新版本 v{updateInfo.latest}</strong>
              <span className="muted-text">
                {installingUpdate
                  ? describeUpdateProgress(updateProgress)
                  : `当前 v${updateInfo.current}，更新后自动重启`}
              </span>
              {installingUpdate && (
                <span className="update-bar" aria-hidden>
                  <span
                    className="update-bar-fill"
                    style={{
                      width: `${
                        updateProgress && updateProgress.total > 0
                          ? Math.min(100, (updateProgress.downloaded / updateProgress.total) * 100)
                          : 10
                      }%`
                    }}
                  />
                </span>
              )}
              {updateError && !installingUpdate && (
                <span className="update-error">更新失败：{updateError}</span>
              )}
            </div>
            <div className="update-card-actions">
              <button className="btn primary small" disabled={installingUpdate} onClick={() => void installUpdate()}>
                {installingUpdate ? '更新中…' : '立即更新'}
              </button>
              <button
                className="btn secondary small"
                disabled={!updateInfo.assetUrl}
                onClick={() => updateInfo.assetUrl && void api.openExternal?.(updateInfo.assetUrl)}
              >
                浏览器下载
              </button>
              <button className="btn-text muted" onClick={() => void api.openUpdateLog?.().catch(() => {})}>
                查看更新日志
              </button>
            </div>
          </div>
        ) : updateInfo ? (
          <Row label="更新状态">
            <span className="muted-text">已是最新版本</span>
          </Row>
        ) : null}
      </Group>

      <Group title="问题反馈">
        <Row
          label="GitHub Token"
          hint="填写一次即可。需要该仓库的 Issues 读写权限（GitHub → Settings → Developer settings → Personal access tokens）。只保存在本机，不会随安装包分发。留空则报告仅保存在本机。"
        >
          <input
            className="input"
            type="password"
            value={token}
            placeholder="github_pat_…"
            aria-label="GitHub Token"
            onChange={(e) => setToken(e.target.value)}
            onBlur={() => {
              if (token.trim() !== (settings.githubToken ?? '')) patch({ githubToken: token.trim() })
            }}
          />
        </Row>
        <Row label="上报仓库" hint="格式为 owner/repo，例如 dearguan/vidgo">
          <input
            className="input"
            value={repo}
            placeholder="dearguan/vidgo"
            aria-label="上报仓库"
            onChange={(e) => setRepo(e.target.value)}
            onBlur={() => {
              if (repo.trim() !== settings.reportRepo) patch({ reportRepo: repo.trim() })
            }}
          />
        </Row>
        <Row label="当前状态" hint="解析失败或下载失败时，点击“上报给开发者”会自动创建 Issue 并附带诊断信息。">
          <span className="muted-text">
            {settings.githubToken ? '已配置 Token，可直接上报' : '未配置 Token，报告只会保存在本机'}
          </span>
        </Row>
      </Group>

      <Group title="关于与诊断">
        <Row label="解析引擎" hint="yt-dlp，随软件内置，无需安装 Python">
          <span className="muted-text">{appInfo?.ytdlp ?? '读取中…'}</span>
        </Row>
        <Row label="合并组件" hint="FFmpeg / FFprobe 内置，用于音视频合并与音频提取">
          <span className="muted-text">已内置</span>
        </Row>
        <Row label="第三方许可" hint="yt-dlp（Unlicense）、FFmpeg（LGPL/GPL）、Electron（MIT）">
          <Icon name="info" size={16} />
        </Row>
      </Group>
    </div>
  )
}
