import { useMemo, useState, type ReactElement } from 'react'
import { Icon } from '../Icon'
import { useApp } from '../state'
import { ACTIVE_STATUS, formatSpeed } from '../utils'
import { TaskCard } from '../components/TaskCard'
import { ConfirmModal } from '../components/Modal'

type Filter = 'all' | 'DOWNLOADING' | 'WAITING' | 'MERGING'
type Sort = 'created' | 'status' | 'size'

export function DownloadingPage(): ReactElement {
  const { tasks, taskAction, setView } = useApp()
  const [filter, setFilter] = useState<Filter>('all')
  const [sort, setSort] = useState<Sort>('created')
  const [selecting, setSelecting] = useState(false)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [confirmDelete, setConfirmDelete] = useState(false)

  const active = tasks.filter((t) => ACTIVE_STATUS.includes(t.status) || t.status === 'PAUSED')
  const list = useMemo(() => {
    const filtered = filter === 'all' ? active : active.filter((t) => t.status === filter)
    return [...filtered].sort((a, b) => {
      if (sort === 'size') return (b.total || 0) - (a.total || 0)
      if (sort === 'status') return a.status.localeCompare(b.status)
      return b.createdAt - a.createdAt
    })
  }, [active, filter, sort])

  const totalSpeed = active.reduce((s, t) => s + (t.status === 'DOWNLOADING' ? t.speed : 0), 0)

  const toggleSelect = (id: string): void => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const batch = (action: 'pause' | 'resume'): void => {
    selected.forEach((id) => taskAction(action, id))
    setSelected(new Set())
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <span className="muted-text">
          {active.length} 个任务{totalSpeed > 0 ? ` · 总速度 ${formatSpeed(totalSpeed)}` : ''}
        </span>
        <div className="toolbar-actions">
          <select className="select small" value={filter} onChange={(e) => setFilter(e.target.value as Filter)} aria-label="筛选">
            <option value="all">全部状态</option>
            <option value="DOWNLOADING">下载中</option>
            <option value="WAITING">排队中</option>
            <option value="MERGING">合并中</option>
          </select>
          <select className="select small" value={sort} onChange={(e) => setSort(e.target.value as Sort)} aria-label="排序">
            <option value="created">按创建时间</option>
            <option value="status">按状态</option>
            <option value="size">按大小</option>
          </select>
          <button
            className={`btn secondary small ${selecting ? 'on' : ''}`}
            onClick={() => {
              setSelecting((v) => !v)
              setSelected(new Set())
            }}
          >
            批量操作
          </button>
        </div>
      </div>

      {list.length === 0 ? (
        <div className="empty-state">
          <Icon name="download" size={48} />
          <h2>当前没有下载任务</h2>
          <p>粘贴视频链接，解析后即可开始下载。</p>
          <button className="btn primary" onClick={() => setView('home')}>
            去下载
          </button>
        </div>
      ) : (
        <div className="task-list">
          {list.map((t) => (
            <TaskCard key={t.id} task={t} selectable={selecting} selected={selected.has(t.id)} onSelect={toggleSelect} />
          ))}
        </div>
      )}

      {selecting && selected.size > 0 && (
        <div className="batch-bar">
          <span>已选 {selected.size} 项</span>
          <button className="btn secondary small" onClick={() => batch('pause')}>
            暂停
          </button>
          <button className="btn secondary small" onClick={() => batch('resume')}>
            继续
          </button>
          <button className="btn danger small" onClick={() => setConfirmDelete(true)}>
            删除
          </button>
        </div>
      )}

      {confirmDelete && (
        <ConfirmModal
          title="删除所选任务"
          body={`将删除 ${selected.size} 个任务及其记录。仅移除记录，不删除已下载的文件。`}
          onClose={() => setConfirmDelete(false)}
          onConfirm={() => {
            selected.forEach((id) => taskAction('remove', id))
            setSelected(new Set())
            setConfirmDelete(false)
          }}
        />
      )}
    </div>
  )
}
