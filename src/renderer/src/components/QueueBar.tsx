import { useEffect, useRef, useState, type ReactElement } from 'react'
import { Icon } from '../Icon'
import { useApp } from '../state'
import { ACTIVE_STATUS, STATUS_LABEL, formatBytes, formatSpeed } from '../utils'
import { StatusBadge, Thumb } from './TaskCard'

export function QueueBar(): ReactElement {
  const { tasks, taskAction, setView } = useApp()
  const active = tasks.filter((t) => ACTIVE_STATUS.includes(t.status) || t.status === 'PAUSED')
  const running = active.filter((t) => t.status !== 'PAUSED')
  const [expanded, setExpanded] = useState(false)
  const manual = useRef<boolean | null>(null)
  const prevCount = useRef(0)

  useEffect(() => {
    if (manual.current === null && running.length >= 3) setExpanded(true)
    if (running.length === 0 && prevCount.current > 0) {
      const timer = window.setTimeout(() => {
        manual.current = null
        setExpanded(false)
      }, 3000)
      prevCount.current = 0
      return () => window.clearTimeout(timer)
    }
    prevCount.current = running.length
  }, [running.length])

  const totalSpeed = active.reduce((sum, t) => sum + (t.status === 'DOWNLOADING' ? t.speed : 0), 0)
  const totalBytes = active.reduce((sum, t) => sum + (t.total || 0), 0)
  const doneBytes = active.reduce((sum, t) => sum + (t.downloaded || 0), 0)
  const overall = totalBytes > 0 ? (doneBytes / totalBytes) * 100 : active.length ? active.reduce((s, t) => s + t.progress, 0) / active.length : 0

  const toggle = (): void => {
    const next = !expanded
    manual.current = next
    setExpanded(next)
  }

  return (
    <div className={`queue-bar ${expanded ? 'expanded' : ''}`}>
      {expanded && active.length > 0 && (
        <div className="queue-list">
          {active.map((t) => (
            <div className="queue-row" key={t.id}>
              <Thumb src={t.thumbnail} />
              <div className="queue-row-body">
                <div className="queue-row-head">
                  <span className="task-title" title={t.title}>
                    {t.title}
                  </span>
                  <StatusBadge status={t.status} />
                </div>
                <div className="task-meta">
                  <span>{t.formatLabel}</span>
                  {t.total > 0 && (
                    <span>
                      {formatBytes(t.downloaded)} / {formatBytes(t.total)}
                    </span>
                  )}
                  {t.speed > 0 && <span>{formatSpeed(t.speed)}</span>}
                </div>
                {t.status !== 'WAITING' && (
                  <div className={`progress ${t.status === 'MERGING' ? 'merging' : ''}`} title={`已完成 ${t.progress.toFixed(1)}%`}>
                    <div className="progress-bar" style={{ width: `${Math.min(100, t.progress)}%` }} />
                  </div>
                )}
              </div>
              <div className="task-actions">
                {t.status === 'PAUSED' ? (
                  <button className="icon-btn" title="继续" aria-label="继续" onClick={() => taskAction('resume', t.id)}>
                    <Icon name="play" />
                  </button>
                ) : (
                  <button className="icon-btn" title="暂停" aria-label="暂停" onClick={() => taskAction('pause', t.id)}>
                    <Icon name="pause" />
                  </button>
                )}
                <button className="icon-btn" title="取消" aria-label="取消" onClick={() => taskAction('cancel', t.id)}>
                  <Icon name="close" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      <div className="queue-summary">
        <button
          className="queue-summary-main"
          onClick={() => (active.length ? toggle() : setView('downloading'))}
          title={active.length ? '展开或收起任务队列' : '查看下载任务'}
        >
          <Icon name="download" size={18} />
          <span>{active.length ? `${active.length} 个任务进行中` : '没有进行中的任务'}</span>
          {totalSpeed > 0 && <span className="queue-speed">{formatSpeed(totalSpeed)}</span>}
          {active.length > 0 && (
            <span className="queue-overall" title={`总进度 ${overall.toFixed(1)}%（按已下载字节估算）`}>
              <span className="queue-overall-bar" style={{ width: `${Math.min(100, overall)}%` }} />
            </span>
          )}
        </button>
        {active.length > 0 && (
          <button className="icon-btn" onClick={toggle} title={expanded ? '收起' : '展开'} aria-label={expanded ? '收起队列' : '展开队列'}>
            <Icon name={expanded ? 'chevronDown' : 'chevronUp'} />
          </button>
        )}
        {active.length === 0 && <span className="queue-idle">{STATUS_LABEL.COMPLETED} {tasks.filter((t) => t.status === 'COMPLETED').length} 个</span>}
      </div>
    </div>
  )
}
