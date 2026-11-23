import { useState, type ReactElement } from 'react'
import { AppProvider, describeUpdateProgress, useApp } from './state'
import { Icon, Logo } from './Icon'
import { ACTIVE_STATUS, type View } from './utils'
import { QueueBar } from './components/QueueBar'
import { HomePage } from './pages/HomePage'
import { DownloadingPage } from './pages/DownloadingPage'
import { CompletedPage } from './pages/CompletedPage'
import { FailedPage } from './pages/FailedPage'
import { LibraryPage } from './pages/LibraryPage'
import { CapturePage } from './pages/CapturePage'
import { SubscriptionsPage } from './pages/SubscriptionsPage'
import { SettingsPage } from './pages/SettingsPage'

const PAGE_TITLE: Record<View, { title: string; sub?: string }> = {
  home: { title: '开始下载', sub: '粘贴网址，解析后选择画质' },
  downloading: { title: '正在下载' },
  completed: { title: '已完成' },
  failed: { title: '失败任务' },
  library: { title: '视频库', sub: '浏览和管理已下载的媒体文件' },
  capture: { title: '网页捕获', sub: '在内置浏览器中发现网页媒体' },
  subscriptions: { title: '订阅', sub: '跟踪收藏页面的最新内容' },
  settings: { title: '设置' }
}

function Shell(): ReactElement {
  const {
    view,
    setView,
    tasks,
    toasts,
    appInfo,
    updateInfo,
    installingUpdate,
    updateProgress,
    installUpdate
  } = useApp()
  const [bannerHidden, setBannerHidden] = useState(false)
  const activeCount = tasks.filter((t) => ACTIVE_STATUS.includes(t.status) || t.status === 'PAUSED').length
  const completedCount = tasks.filter((t) => t.status === 'COMPLETED').length
  const failedCount = tasks.filter((t) => t.status === 'FAILED' || t.status === 'CANCELLED').length

  const groups: Array<{
    label: string
    items: Array<{ id: View; label: string; icon: string; count?: number }>
  }> = [
    {
      label: '下载',
      items: [
        { id: 'home', label: '开始下载', icon: 'download' },
        { id: 'downloading', label: '正在下载', icon: 'clock', count: activeCount },
        { id: 'completed', label: '已完成', icon: 'checkCircle', count: completedCount },
        { id: 'failed', label: '失败任务', icon: 'alert', count: failedCount }
      ]
    },
    {
      label: '媒体',
      items: [
        { id: 'library', label: '视频库', icon: 'library' },
        { id: 'capture', label: '网页捕获', icon: 'globe' }
      ]
    },
    {
      label: '其他',
      items: [{ id: 'subscriptions', label: '订阅', icon: 'rss' }]
    }
  ]

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <Logo size={34} />
          <div>
            <div className="brand-name">Vidgo</div>
            <div className="brand-sub">视频下载器</div>
          </div>
        </div>
        <nav className="nav">
          {groups.map((g) => (
            <div className="nav-group" key={g.label}>
              <div className="nav-group-label">{g.label}</div>
              {g.items.map((item) => (
                <button
                  key={item.id}
                  className={`nav-item ${view === item.id ? 'active' : ''}`}
                  onClick={() => setView(item.id)}
                >
                  <Icon name={item.icon} size={20} />
                  <span>{item.label}</span>
                  {item.count ? <span className="nav-count">{item.count}</span> : null}
                </button>
              ))}
            </div>
          ))}
        </nav>
        <div className="sidebar-foot">
          <button className={`nav-item ${view === 'settings' ? 'active' : ''}`} onClick={() => setView('settings')}>
            <Icon name="settings" size={20} />
            <span>设置</span>
          </button>
          <div className="version">v{appInfo?.version ?? '0.2.0'}</div>
        </div>
      </aside>

      <div className="content">
        <header className="topbar">
          <div className="topbar-title">
            <h1>{PAGE_TITLE[view].title}</h1>
            {PAGE_TITLE[view].sub && <span>{PAGE_TITLE[view].sub}</span>}
          </div>
        </header>

        {updateInfo?.available && !bannerHidden && (
          <div className="update-banner">
            <Icon name="download" size={16} />
            <span className="update-banner-text">
              发现新版本 v{updateInfo.latest}
              {installingUpdate ? `，${describeUpdateProgress(updateProgress)}` : ''}
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
            <button className="btn primary small" disabled={installingUpdate} onClick={() => void installUpdate()}>
              {installingUpdate ? '更新中…' : '立即更新'}
            </button>
            <button className="btn-text muted" onClick={() => setBannerHidden(true)}>
              稍后
            </button>
          </div>
        )}

        <main className="page-scroll">
          {view === 'home' && <HomePage />}
          {view === 'downloading' && <DownloadingPage />}
          {view === 'completed' && <CompletedPage />}
          {view === 'failed' && <FailedPage />}
          {view === 'library' && <LibraryPage />}
          {view === 'capture' && <CapturePage />}
          <div hidden={view !== 'subscriptions'}>
            <SubscriptionsPage />
          </div>
          {view === 'settings' && <SettingsPage />}
        </main>

        <QueueBar />
      </div>

      <div className="toast-stack">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.type}`}>
            {t.text}
          </div>
        ))}
      </div>
    </div>
  )
}

export default function App(): ReactElement {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  )
}
