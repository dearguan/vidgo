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
  const { settings, appInfo, updateSettings, chooseDir, openBuiltinBrowser } = useApp()
  const [proxy, setProxy] = useState('')

  useEffect(() => {
    setProxy(settings?.proxy ?? '')
  }, [settings?.proxy])

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
          hint="最省心的方式：点击按钮打开内置浏览器，登录抖音、B站、YouTube 等视频站，Vidgo 自动复用登录态。登录一次，下次打开还在。"
        >
          <button className="btn primary" onClick={() => void openBuiltinBrowser()}>
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

      <Group title="关于与诊断">
        <Row label="软件版本">
          <span className="muted-text">Vidgo v{appInfo?.version ?? '0.2.0'}</span>
        </Row>
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
