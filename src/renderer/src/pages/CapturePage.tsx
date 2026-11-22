import { useState, type ReactElement } from 'react'
import { api } from '../api'
import { Icon } from '../Icon'
import { useApp } from '../state'
import { formatTime, isHttpUrl } from '../utils'

export function CapturePage(): ReactElement {
  const { sniffedMedia, clearSniffedMedia, requestParse, notify } = useApp()
  const [url, setUrl] = useState('')
  const [browserOpen, setBrowserOpen] = useState(false)
  const [opening, setOpening] = useState(false)
  const valid = isHttpUrl(url)

  const openPage = async (): Promise<void> => {
    const target = url.trim()
    if (!isHttpUrl(target) || opening) return
    setOpening(true)
    try {
      await api.openBuiltinBrowser(target)
      setBrowserOpen(true)
    } catch (e) {
      notify(e instanceof Error ? e.message : String(e), 'err')
    } finally {
      setOpening(false)
    }
  }

  const reload = async (): Promise<void> => {
    try {
      await api.reloadBuiltinBrowser()
    } catch (e) {
      notify(e instanceof Error ? e.message : String(e), 'err')
    }
  }

  const close = async (): Promise<void> => {
    try {
      await api.closeBuiltinBrowser()
      setBrowserOpen(false)
    } catch (e) {
      notify(e instanceof Error ? e.message : String(e), 'err')
    }
  }

  return (
    <div className="page">
      <div className="card capture-controls">
        <div className="url-input">
          <Icon name="globe" size={20} />
          <input
            value={url}
            placeholder="输入要捕获的网页地址，例如 https://…"
            aria-label="捕获网页地址"
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void openPage()
            }}
          />
          {url && (
            <button className="icon-btn" title="清空网址" aria-label="清空网址" onClick={() => setUrl('')}>
              <Icon name="close" size={14} />
            </button>
          )}
        </div>
        <button className="btn primary" disabled={!valid || opening} onClick={() => void openPage()}>
          {opening ? '打开中…' : browserOpen ? '跳转' : '打开网页'}
        </button>
        <button className="btn secondary" disabled={!browserOpen} onClick={() => void reload()}>
          <Icon name="refresh" size={15} />
          刷新
        </button>
        <button className="btn secondary" disabled={!browserOpen} onClick={() => void close()}>
          关闭
        </button>
      </div>

      <div className="page-toolbar capture-summary">
        <span className="muted-text">已捕获 {sniffedMedia.length} 个媒体地址，最多保留 200 个</span>
        <button className="btn secondary small" disabled={sniffedMedia.length === 0} onClick={clearSniffedMedia}>
          <Icon name="trash" size={15} />
          清空列表
        </button>
      </div>

      {sniffedMedia.length === 0 ? (
        <div className="empty-state">
          <Icon name="globe" size={48} />
          <h2>还没有捕获到媒体</h2>
          <p>打开网页并播放视频或音频，检测到的媒体地址会显示在这里。</p>
        </div>
      ) : (
        <div className="task-list">
          {sniffedMedia.map((media) => (
            <div className="task-card capture-card" key={`${media.type}:${media.url}`}>
              <div className="file-icon">
                <Icon name={media.type === 'mp3' || media.type === 'm4a' ? 'music' : 'video'} size={24} />
              </div>
              <div className="task-body">
                <div className="task-head">
                  <span className="task-title" title={media.title || media.url}>
                    {media.title || media.url}
                  </span>
                  <span className="badge status-downloading">{media.type.toUpperCase()}</span>
                </div>
                <div className="task-meta">
                  <span>{formatTime(media.timestamp)}</span>
                  {media.pageUrl && <span title={media.pageUrl}>来源：{media.pageUrl}</span>}
                </div>
                <div className="media-url" title={media.url}>
                  {media.url}
                </div>
              </div>
              <div className="task-actions">
                <button className="btn primary small" onClick={() => requestParse(media.url)}>
                  <Icon name="download" size={15} />
                  导入解析
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
