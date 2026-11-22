import { useCallback, useEffect, useState, type ReactElement } from 'react'
import type { LibraryFile } from '../../../shared/types'
import { api } from '../api'
import { Icon } from '../Icon'
import { useApp } from '../state'
import { formatBytes, formatTime } from '../utils'
import { ConfirmModal } from '../components/Modal'

export function LibraryPage(): ReactElement {
  const { notify } = useApp()
  const [files, setFiles] = useState<LibraryFile[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [deleteTarget, setDeleteTarget] = useState<LibraryFile | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      setFiles(await api.listLibraryFiles())
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const runAction = async (action: () => Promise<void>): Promise<void> => {
    try {
      await action()
    } catch (e) {
      notify(e instanceof Error ? e.message : String(e), 'err')
    }
  }

  const deleteFile = async (): Promise<void> => {
    if (!deleteTarget) return
    try {
      await api.deleteLibraryFile(deleteTarget.path)
      setFiles((list) => list.filter((file) => file.path !== deleteTarget.path))
      notify(`已删除 ${deleteTarget.name}`)
      setDeleteTarget(null)
    } catch (e) {
      notify(e instanceof Error ? e.message : String(e), 'err')
    }
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <span className="muted-text">{loading ? '正在读取视频库…' : `${files.length} 个文件`}</span>
        <button className="btn secondary small" disabled={loading} onClick={() => void load()}>
          <Icon name="refresh" size={15} className={loading ? 'spin' : undefined} />
          刷新
        </button>
      </div>

      {loading ? (
        <div className="empty-state">
          <Icon name="spinner" size={42} className="spin" />
          <h2>正在读取视频库</h2>
        </div>
      ) : error ? (
        <div className="empty-state">
          <Icon name="alert" size={48} />
          <h2>无法读取视频库</h2>
          <p>{error}</p>
          <button className="btn primary" onClick={() => void load()}>
            重试
          </button>
        </div>
      ) : files.length === 0 ? (
        <div className="empty-state">
          <Icon name="library" size={48} />
          <h2>视频库还是空的</h2>
          <p>下载目录中的媒体文件会显示在这里。</p>
          <button className="btn secondary" onClick={() => void load()}>
            重新扫描
          </button>
        </div>
      ) : (
        <div className="task-list">
          {files.map((file) => (
            <div className="task-card" key={file.path}>
              <div className="file-icon">
                <Icon name={file.extension.toLowerCase() === 'mp3' || file.extension.toLowerCase() === 'm4a' ? 'music' : 'video'} size={24} />
              </div>
              <div className="task-body">
                <div className="task-head">
                  <span className="task-title" title={file.name}>
                    {file.name}
                  </span>
                  <span className="badge">{file.extension.replace(/^\./, '').toUpperCase() || '文件'}</span>
                </div>
                <div className="task-meta">
                  <span>{formatBytes(file.size)}</span>
                  <span>{formatTime(file.modifiedAt)}</span>
                </div>
                <div className="file-path" title={file.path}>
                  {file.path}
                </div>
              </div>
              <div className="task-actions">
                <button
                  className="icon-btn"
                  title="打开文件"
                  aria-label={`打开 ${file.name}`}
                  onClick={() => void runAction(() => api.openLibraryFile(file.path))}
                >
                  <Icon name="play" />
                </button>
                <button
                  className="icon-btn"
                  title="打开所在目录"
                  aria-label={`打开 ${file.name} 所在目录`}
                  onClick={() => void runAction(() => api.openLibraryFolder(file.path))}
                >
                  <Icon name="folder" />
                </button>
                <button
                  className="icon-btn danger"
                  title="删除文件"
                  aria-label={`删除 ${file.name}`}
                  onClick={() => setDeleteTarget(file)}
                >
                  <Icon name="trash" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {deleteTarget && (
        <ConfirmModal
          title="删除媒体文件"
          body={
            <>
              <p>确定永久删除“{deleteTarget.name}”吗？此操作无法撤销。</p>
              <p className="confirm-path">{deleteTarget.path}</p>
            </>
          }
          onClose={() => setDeleteTarget(null)}
          onConfirm={() => void deleteFile()}
        />
      )}
    </div>
  )
}
