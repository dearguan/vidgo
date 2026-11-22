import type { ReactElement } from 'react'
import type { DownloadTask } from '../../../shared/types'
import { Icon } from '../Icon'
import { useApp } from '../state'
import { STATUS_LABEL, formatBytes, formatEta, formatSpeed, formatTime } from '../utils'

export function Thumb({ src, size = 'small' }: { src?: string; size?: 'small' | 'large' }): ReactElement {
  // B 站等站点返回 http:// 缩略图，WebView 可能按混合内容拦截，统一升级为 https
  const safeSrc = src?.startsWith('http://') ? `https://${src.slice('http://'.length)}` : src
  return (
    <div className={`thumb ${size}`}>
      <span className="thumb-fallback">
        <Icon name="video" size={size === 'large' ? 28 : 20} />
      </span>
      {safeSrc ? <img src={safeSrc} alt="" loading="lazy" onError={(e) => (e.currentTarget.style.display = 'none')} /> : null}
    </div>
  )
}

export function StatusBadge({ status }: { status: DownloadTask['status'] }): ReactElement {
  return <span className={`badge status-${status.toLowerCase()}`}>{STATUS_LABEL[status]}</span>
}

interface TaskCardProps {
  task: DownloadTask
  onDetail?: (task: DownloadTask) => void
  selectable?: boolean
  selected?: boolean
  onSelect?: (id: string) => void
}

export function TaskCard({ task, onDetail, selectable, selected, onSelect }: TaskCardProps): ReactElement {
  const { taskAction, loginFor } = useApp()
  const s = task.status
  const active = s === 'DOWNLOADING' || s === 'MERGING'
  const showProgress = active || s === 'PAUSED'
  const pct = Math.min(100, Math.max(0, task.progress || 0))

  return (
    <div className={`task-card ${s === 'CANCELLED' ? 'dim' : ''}`}>
      {selectable ? (
        <label className="task-check" onClick={(e) => e.stopPropagation()}>
          <input type="checkbox" checked={selected} onChange={() => onSelect?.(task.id)} aria-label="选择任务" />
        </label>
      ) : null}
      <Thumb src={task.thumbnail} />
      <div className="task-body">
        <div className="task-head">
          <span className="task-title" title={task.title}>
            {task.title}
          </span>
          <StatusBadge status={s} />
        </div>
        <div className="task-meta">
          <span>{task.formatLabel}</span>
          {(active || s === 'PAUSED') && task.total > 0 && (
            <span>
              {formatBytes(task.downloaded)} / {formatBytes(task.total)}
            </span>
          )}
          {active && task.downloaded === 0 && !task.total && <span>连接中…</span>}
          {active && task.speed > 0 && <span>{formatSpeed(task.speed)}</span>}
          {active && <span>{formatEta(task.eta)}</span>}
          {s === 'WAITING' && <span>等待空闲下载位置</span>}
          {s === 'COMPLETED' && <span>{formatBytes(task.total)}</span>}
          {s === 'COMPLETED' && <span>{formatTime(task.completedAt)}</span>}
        </div>
        {showProgress && (
          <div
            className={`progress ${s === 'MERGING' ? 'merging' : ''}`}
            title={`已完成 ${pct.toFixed(1)}%`}
            role="progressbar"
            aria-valuenow={Math.round(pct)}
          >
            <div className="progress-bar" style={{ width: `${pct}%` }} />
          </div>
        )}
        {(s === 'FAILED' || s === 'CANCELLED') && task.error && (
          <div className="task-error">
            <Icon name="alert" size={14} />
            <span className="task-error-text" title={task.error}>
              {task.error}
            </span>
            {task.needsLogin && (
              <button className="btn-text" onClick={() => void loginFor(task.url, task.id)}>
                去登录
              </button>
            )}
            {onDetail && (
              <button className="btn-text" onClick={() => onDetail(task)}>
                查看详情
              </button>
            )}
          </div>
        )}
      </div>
      <div className="task-actions">
        {(active || s === 'WAITING') && (
          <button className="icon-btn" title="暂停" aria-label="暂停" onClick={() => taskAction('pause', task.id)}>
            <Icon name="pause" />
          </button>
        )}
        {s === 'PAUSED' && (
          <button className="icon-btn" title="继续" aria-label="继续" onClick={() => taskAction('resume', task.id)}>
            <Icon name="play" />
          </button>
        )}
        {(s === 'FAILED' || s === 'CANCELLED') && (
          <button className="icon-btn" title="重试" aria-label="重试" onClick={() => taskAction('resume', task.id)}>
            <Icon name="refresh" />
          </button>
        )}
        {s === 'COMPLETED' && task.filePath && (
          <button className="icon-btn" title="播放" aria-label="播放" onClick={() => taskAction('openFile', task.id)}>
            <Icon name="play" />
          </button>
        )}
        {(s === 'COMPLETED' || s === 'FAILED') && (
          <button
            className="icon-btn"
            title="打开文件夹"
            aria-label="打开文件夹"
            onClick={() => taskAction('openFolder', task.id)}
          >
            <Icon name="folder" />
          </button>
        )}
        {(active || s === 'WAITING' || s === 'PAUSED') && (
          <button className="icon-btn" title="取消" aria-label="取消" onClick={() => taskAction('cancel', task.id)}>
            <Icon name="close" />
          </button>
        )}
        {(s === 'COMPLETED' || s === 'FAILED' || s === 'CANCELLED') && (
          <>
            <button
              className="icon-btn"
              title="重新下载"
              aria-label="重新下载"
              onClick={() => taskAction('redownload', task.id)}
            >
              <Icon name="refresh" />
            </button>
            <button
              className="icon-btn danger"
              title="删除记录"
              aria-label="删除记录"
              onClick={() => taskAction('remove', task.id)}
            >
              <Icon name="trash" />
            </button>
          </>
        )}
      </div>
    </div>
  )
}
