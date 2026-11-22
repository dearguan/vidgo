import { useEffect, useState, type ReactElement, type ReactNode } from 'react'
import type { Settings, ThemeMode } from '../../../shared/types'
import { Icon } from '../Icon'
import { useApp } from '../state'

type CookieBrowser = Settings['cookieBrowser']

const BROWSER_OPTIONS: Array<{ value: CookieBrowser; label: string; hint?: string }> = [
  { value: 'auto', label: '自动检测（推荐）', hint: '依次尝试系统中可用的浏览器 Cookie' },
  { value: 'chrome', label: 'Google Chrome' },
  { value: 'edge', label: 'Microsoft Edge' },
  { value: 'brave', label: 'Brave' },
  { value: 'firefox', label: 'Firefox' },
  { value: 'safari', label: 'Safari' },
  { value: 'vivaldi', label: 'Vivaldi' },
  { value: 'opera', label: 'Opera' },
  { value: 'none', label: '不读取' }
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
    notify,
    updateInfo,
    checkingUpdate,
    installingUpdate,
    updateProgress,
    checkUpdate,
    installUpdate
  } = useApp()
  const [proxy, setProxy] = useState('')
  const [token, setToken] = useState('')
  const [repo, setRepo] = useState('')

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

      <Group title="登录与 Cookie" onReset={() => patch({ cookieBrowser: 'auto' })}>
        <Row
          label="内置浏览器"
          hint="在内置浏览器里登录后，点页面右上角「完成登录」，该站 Cookie 会保存并自动用于解析与下载（有效期 7 天）。也可以直接读取系统浏览器 Cookie。"
        >
          <button
            className="btn primary"
            onClick={() => {
              void openBuiltinBrowser().catch((e) => notify(e instanceof Error ? e.message : String(e), 'err'))
            }}
          >
            打开内置浏览器登录
          </button>
        </Row>
        <Row
          label="读取系统浏览器 Cookie"
          hint="如果你的 Chrome/Edge 里已经登录过视频站，Vidgo 可以直接读那个浏览器的 Cookie，零额外配置即可下载。"
        >
          <select
            className="input select"
            value={settings.cookieBrowser}
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
                {installingUpdate && updateProgress && updateProgress.total > 0
                  ? `正在下载 ${Math.min(100, Math.round((updateProgress.downloaded / updateProgress.total) * 100))}%`
                  : `当前 v${updateInfo.current}，更新后自动重启`}
              </span>
            </div>
            <button className="btn primary small" disabled={installingUpdate} onClick={() => void installUpdate()}>
              {installingUpdate ? '更新中…' : '立即更新'}
            </button>
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
