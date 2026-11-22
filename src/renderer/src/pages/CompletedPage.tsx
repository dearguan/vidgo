import { useState, type ReactElement } from 'react'
import { Icon } from '../Icon'
import { useApp } from '../state'
import { formatBytes } from '../utils'
import { TaskCard, Thumb } from '../components/TaskCard'
import { ConfirmModal } from '../components/Modal'

export function CompletedPage(): ReactElement {
  const { tasks, taskAction, notify } = useApp()
  const [mode, setMode] = useState<'grid' | 'list'>('grid')
  const [confirmClear, setConfirmClear] = useState(false)
  const completed = tasks.filter((t) => t.status === 'COMPLETED')

  return (
    <div className="page">
      <div className="page-toolbar">
        <span className="muted-text">{completed.length} 个文件</span>
        <div className="toolbar-actions">
          <div className="view-toggle" role="group" aria-label="视图切换">
            <button className={`icon-btn ${mode === 'grid' ? 'on' : ''}`} title="网格视图" aria-label="网格视图" onClick={() => setMode('grid')}>
              <Icon name="grid" size={16} />
            </button>
            <button className={`icon-btn ${mode === 'list' ? 'on' : ''}`} title="列表视图" aria-label="列表视图" onClick={() => setMode('list')}>
              <Icon name="list" size={16} />
            </button>
          </div>
          <button className="btn secondary small" onClick={() => void window.api.openSaveDir()}>
            <Icon name="folder" size={15} />
            打开下载目录
          </button>
          {completed.length > 0 && (
            <button className="btn secondary small" onClick={() => setConfirmClear(true)}>
              清理记录
            </button>
          )}
        </div>
      </div>

      {completed.length === 0 ? (
        <div className="empty-state">
          <Icon name="checkCircle" size={48} />
          <h2>还没有下载完成的文件</h2>
          <p>下载完成的视频会显示在这里。</p>
          <button className="btn primary" onClick={() => void window.api.openSaveDir()}>
            打开下载目录
          </button>
        </div>
      ) : mode === 'list' ? (
        <div className="task-list">
          {completed.map((t) => (
            <TaskCard key={t.id} task={t} />
          ))}
        </div>
      ) : (
        <div className="media-grid">
          {completed.map((t) => (
            <div className="grid-card" key={t.id}>
              <div className="grid-thumb">
                <Thumb src={t.thumbnail} size="large" />
                <div className="grid-hover">
                  <button
                    className="icon-btn light"
                    title="播放"
                    aria-label="播放"
                    onClick={() => taskAction('openFile', t.id)}
                  >
                    <Icon name="play" />
                  </button>
                  <button
                    className="icon-btn light"
                    title="打开所在文件夹"
                    aria-label="打开所在文件夹"
                    onClick={() => taskAction('openFolder', t.id)}
                  >
                    <Icon name="folder" />
                  </button>
                  <button
                    className="icon-btn light"
                    title="重新下载"
                    aria-label="重新下载"
                    onClick={() => {
                      taskAction('redownload', t.id)
                      notify('已加入下载队列')
                    }}
                  >
                    <Icon name="refresh" />
                  </button>
                </div>
              </div>
              <div className="grid-info">
                <div className="task-title" title={t.title}>
                  {t.title}
                </div>
                <div className="task-meta">
                  <span>{t.formatLabel}</span>
                  <span>{formatBytes(t.total)}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {confirmClear && (
        <ConfirmModal
          title="清理已完成记录"
          body={`将移除 ${completed.length} 条下载记录。仅移除记录，不删除文件。`}
          confirmText="清理"
          withDeleteFile
          onClose={() => setConfirmClear(false)}
          onConfirm={(deleteFile) => {
            completed.forEach((t) => taskAction('remove', t.id, deleteFile))
            setConfirmClear(false)
            notify(deleteFile ? '记录与文件已删除' : '记录已清理，文件保留')
          }}
        />
      )}
    </div>
  )
}
