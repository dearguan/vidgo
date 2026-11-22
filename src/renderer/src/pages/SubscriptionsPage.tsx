import { useCallback, useEffect, useRef, useState, type ReactElement } from 'react'
import type { Subscription } from '../../../shared/types'
import { api } from '../api'
import { Icon } from '../Icon'
import { useApp } from '../state'
import { formatDuration, formatTime, isHttpUrl } from '../utils'
import { ConfirmModal } from '../components/Modal'
import { Thumb } from '../components/TaskCard'

type CheckMode = 'single' | 'all' | 'auto'

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

function uploadDateText(value?: string): string {
  if (!value) return ''
  if (/^\d{8}$/.test(value)) return `${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}`
  return value
}

export function SubscriptionsPage(): ReactElement {
  const { notify, requestParse } = useApp()
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([])
  const [url, setUrl] = useState('')
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState('')
  const [adding, setAdding] = useState(false)
  const [checkMode, setCheckMode] = useState<CheckMode | null>(null)
  const [checkingId, setCheckingId] = useState<string | null>(null)
  const [newEntryIds, setNewEntryIds] = useState<Record<string, string[]>>({})
  const [deleteTarget, setDeleteTarget] = useState<Subscription | null>(null)
  const checkLock = useRef(false)
  const subscriptionsRef = useRef<Subscription[]>([])
  const valid = isHttpUrl(url)

  useEffect(() => {
    subscriptionsRef.current = subscriptions
  }, [subscriptions])

  const load = useCallback(async () => {
    setLoading(true)
    setLoadError('')
    try {
      const list = await api.listSubscriptions()
      subscriptionsRef.current = list
      setSubscriptions(list)
    } catch (e) {
      setLoadError(errorText(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const runChecks = useCallback(
    async (ids: string[], mode: CheckMode) => {
      if (ids.length === 0) return
      if (checkLock.current) {
        if (mode !== 'auto') notify('订阅检查正在进行，请稍候', 'err')
        return
      }

      checkLock.current = true
      setCheckMode(mode)
      let added = 0
      let failed = 0
      for (const id of ids) {
        setCheckingId(id)
        try {
          const result = await api.checkSubscription(id)
          setSubscriptions((list) => list.map((item) => (item.id === id ? result.subscription : item)))
          setNewEntryIds((current) => ({ ...current, [id]: result.newEntryIds }))
          added += result.newEntryIds.length
        } catch (e) {
          failed++
          notify(`检查订阅失败：${errorText(e)}`, 'err')
        }
      }
      setCheckingId(null)
      setCheckMode(null)
      checkLock.current = false

      if (mode === 'single' && failed === 0) notify(added > 0 ? `发现 ${added} 个新条目` : '没有新条目')
      if (mode === 'all') {
        notify(failed > 0 ? `检查完成，${failed} 个订阅失败` : added > 0 ? `检查完成，发现 ${added} 个新条目` : '检查完成，没有新条目')
      }
    },
    [notify]
  )

  useEffect(() => {
    const timer = window.setInterval(() => {
      void runChecks(subscriptionsRef.current.map((item) => item.id), 'auto')
    }, 30 * 60 * 1000)
    return () => window.clearInterval(timer)
  }, [runChecks])

  const add = async (): Promise<void> => {
    const target = url.trim()
    if (!isHttpUrl(target) || adding) return
    setAdding(true)
    try {
      const subscription = await api.addSubscription(target)
      setSubscriptions((list) => [subscription, ...list.filter((item) => item.id !== subscription.id)])
      setUrl('')
      notify('订阅已添加')
    } catch (e) {
      notify(errorText(e), 'err')
    } finally {
      setAdding(false)
    }
  }

  const remove = async (): Promise<void> => {
    if (!deleteTarget) return
    try {
      await api.deleteSubscription(deleteTarget.id)
      setSubscriptions((list) => list.filter((item) => item.id !== deleteTarget.id))
      setNewEntryIds((current) => {
        const next = { ...current }
        delete next[deleteTarget.id]
        return next
      })
      setDeleteTarget(null)
      notify('订阅已删除')
    } catch (e) {
      notify(errorText(e), 'err')
    }
  }

  return (
    <div className="page">
      <div className="card subscription-add">
        <div className="url-input">
          <Icon name="rss" size={20} />
          <input
            value={url}
            placeholder="输入频道、播放列表或作者页面地址"
            aria-label="订阅地址"
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void add()
            }}
          />
        </div>
        <button className="btn primary" disabled={!valid || adding} onClick={() => void add()}>
          <Icon name="plus" size={16} />
          {adding ? '添加中…' : '添加订阅'}
        </button>
      </div>

      <div className="page-toolbar subscription-toolbar">
        <span className="muted-text">
          {subscriptions.length} 个订阅{checkMode && checkMode !== 'single' ? ' · 正在顺序检查' : ' · 每 30 分钟自动检查'}
        </span>
        <button
          className="btn secondary small"
          disabled={subscriptions.length === 0 || checkMode !== null}
          onClick={() => void runChecks(subscriptions.map((item) => item.id), 'all')}
        >
          <Icon name="refresh" size={15} className={checkMode === 'all' || checkMode === 'auto' ? 'spin' : undefined} />
          {checkMode === 'all' || checkMode === 'auto' ? '检查中…' : '全部检查'}
        </button>
      </div>

      {loading ? (
        <div className="empty-state">
          <Icon name="spinner" size={42} className="spin" />
          <h2>正在读取订阅</h2>
        </div>
      ) : loadError ? (
        <div className="empty-state">
          <Icon name="alert" size={48} />
          <h2>无法读取订阅</h2>
          <p>{loadError}</p>
          <button className="btn primary" onClick={() => void load()}>
            重试
          </button>
        </div>
      ) : subscriptions.length === 0 ? (
        <div className="empty-state">
          <Icon name="rss" size={48} />
          <h2>还没有订阅</h2>
          <p>添加频道、播放列表或作者页面，新内容会显示在这里。</p>
        </div>
      ) : (
        <div className="subscription-list">
          {subscriptions.map((subscription) => (
            <section className="card subscription-card" key={subscription.id}>
              <div className="subscription-head">
                <div className="subscription-heading">
                  <h2 title={subscription.title}>{subscription.title}</h2>
                  <div className="subscription-url" title={subscription.url}>
                    {subscription.url}
                  </div>
                  <div className="task-meta">
                    <span>添加于 {formatTime(subscription.createdAt)}</span>
                    {subscription.lastCheckedAt && <span>上次检查 {formatTime(subscription.lastCheckedAt)}</span>}
                  </div>
                </div>
                <div className="task-actions">
                  <button
                    className="btn secondary small"
                    disabled={checkMode !== null}
                    onClick={() => void runChecks([subscription.id], 'single')}
                  >
                    <Icon name="refresh" size={15} className={checkingId === subscription.id ? 'spin' : undefined} />
                    {checkingId === subscription.id ? '检查中…' : '检查'}
                  </button>
                  <button className="icon-btn danger" title="删除订阅" aria-label={`删除订阅 ${subscription.title}`} onClick={() => setDeleteTarget(subscription)}>
                    <Icon name="trash" />
                  </button>
                </div>
              </div>

              {subscription.entries.length === 0 ? (
                <div className="empty-state compact">
                  <p>暂无条目，点击检查获取最新内容。</p>
                </div>
              ) : (
                <div className="task-list subscription-entries">
                  {subscription.entries.map((entry) => {
                    const isNew = newEntryIds[subscription.id]?.includes(entry.entryId) ?? false
                    return (
                      <div className="task-card subscription-entry" key={entry.entryId}>
                        <Thumb src={entry.thumbnail} />
                        <div className="task-body">
                          <div className="task-head">
                            <span className="task-title" title={entry.title}>
                              {entry.title}
                            </span>
                            {isNew && <span className="badge status-downloading">新增</span>}
                          </div>
                          <div className="task-meta">
                            {entry.duration && <span>{formatDuration(entry.duration)}</span>}
                            {entry.uploadDate && <span>{uploadDateText(entry.uploadDate)}</span>}
                            <span>收录于 {formatTime(entry.firstSeenAt)}</span>
                          </div>
                        </div>
                        <div className="task-actions">
                          <button className="btn primary small" onClick={() => requestParse(entry.url)}>
                            <Icon name="download" size={15} />
                            下载
                          </button>
                        </div>
                      </div>
                    )
                  })}
                </div>
              )}
            </section>
          ))}
        </div>
      )}

      {deleteTarget && (
        <ConfirmModal
          title="删除订阅"
          body={`确定删除“${deleteTarget.title}”吗？订阅和新增标记将被移除，已下载文件不受影响。`}
          onClose={() => setDeleteTarget(null)}
          onConfirm={() => void remove()}
        />
      )}
    </div>
  )
}
