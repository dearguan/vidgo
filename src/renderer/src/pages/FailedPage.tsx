import { useState, type ReactElement } from 'react'
import type { DownloadTask, ReportInput } from '../../../shared/types'
import { Icon } from '../Icon'
import { useApp } from '../state'
import { ERROR_KIND_LABEL, classifyError, formatTime, type ErrorKind } from '../utils'
import { TaskCard } from '../components/TaskCard'
import { Modal } from '../components/Modal'
import { ReportDialog } from '../components/ReportDialog'

const REASON: Record<ErrorKind, string> = {
  network: '可能是网络连接中断、代理不可用或目标网站响应缓慢。',
  parse: '可能是网站更新导致解析器暂时失效，或该内容需要登录后才能访问。',
  disk: '目标磁盘剩余空间不足，无法继续写入文件。',
  permission: '当前没有写入目标文件夹的权限。',
  protected: '检测到受保护内容，本软件不支持该类型。',
  other: '下载引擎返回了未分类错误，可根据技术详情进一步判断。'
}

function DiagnosisModal({
  task,
  onClose,
  onReport
}: {
  task: DownloadTask
  onClose: () => void
  onReport: () => void
}): ReactElement {
  const { taskAction, notify, setView } = useApp()
  const [showDetail, setShowDetail] = useState(false)
  const kind = classifyError(task.error)
  const log = `${task.error ?? ''}\n时间：${formatTime(task.createdAt)}\n${task.errorDetail ?? ''}`

  return (
    <Modal title="错误诊断" width={560} onClose={onClose}>
      <div className="diag-section">
        <h4>结论</h4>
        <p className="diag-conclusion">{task.error || '下载失败'}</p>
      </div>
      <div className="diag-section">
        <h4>可能原因</h4>
        <p>{REASON[kind]}</p>
      </div>
      <div className="diag-section">
        <h4>建议操作</h4>
        <div className="diag-actions">
          <button
            className="btn primary small"
            onClick={() => {
              taskAction('resume', task.id)
              onClose()
            }}
          >
            重试
          </button>
          {(kind === 'network' || kind === 'disk' || kind === 'permission') && (
            <button
              className="btn secondary small"
              onClick={() => {
                setView('settings')
                onClose()
              }}
            >
              {kind === 'network' ? '检查代理设置' : '更改保存位置'}
            </button>
          )}
          <button
            className="btn secondary small"
            onClick={async () => {
              await navigator.clipboard.writeText(log)
              notify('日志已复制')
            }}
          >
            复制日志
          </button>
          <button className="btn secondary small" onClick={onReport}>
            上报给开发者
          </button>
        </div>
      </div>
      <div className="diag-section">
        <button className="btn-text" onClick={() => setShowDetail((v) => !v)}>
          技术详情
          <Icon name={showDetail ? 'chevronUp' : 'chevronDown'} size={14} />
        </button>
        {showDetail && (
          <pre className="tech-detail">
            {`错误类型：${ERROR_KIND_LABEL[kind]}\n时间：${formatTime(task.createdAt)}\n来源：${task.url}\n\n${task.errorDetail || '无更多日志'}`}
          </pre>
        )}
      </div>
    </Modal>
  )
}

export function FailedPage(): ReactElement {
  const { tasks, taskAction, notify } = useApp()
  const [kind, setKind] = useState<ErrorKind | 'all'>('all')
  const [detail, setDetail] = useState<DownloadTask | null>(null)
  const [reportTask, setReportTask] = useState<DownloadTask | null>(null)
  const [batchInput, setBatchInput] = useState<ReportInput | null>(null)

  const failed = tasks.filter((t) => t.status === 'FAILED' || t.status === 'CANCELLED')
  const list = kind === 'all' ? failed : failed.filter((t) => classifyError(t.error) === kind)
  const kinds = Array.from(new Set(failed.map((t) => classifyError(t.error))))

  const retryAll = (): void => {
    list.forEach((t) => taskAction('resume', t.id))
    notify(`已重试 ${list.length} 个任务`)
  }

  const copyAll = async (): Promise<void> => {
    const text = list.map((t) => `【${t.title}】\n${t.error ?? ''}\n${t.errorDetail ?? ''}`).join('\n\n------\n\n')
    await navigator.clipboard.writeText(text)
    notify('已复制全部日志')
  }

  const reportAll = (): void => {
    const detail = list
      .slice(0, 20)
      .map((t, index) => `${index + 1}. ${t.title}\n链接：${t.url}\n错误：${t.error ?? ''}\n${t.errorDetail ?? ''}`)
      .join('\n\n')
    const more = list.length > 20 ? `\n\n（仅包含前 20 条，共 ${list.length} 条）` : ''
    setBatchInput({
      kind: 'download',
      title: `[批量] ${list.length} 个下载任务失败`,
      message: `${list.length} 个任务下载失败，请协助批量排查。`,
      detail: `${detail}${more}`
    })
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <div className="chip-group">
          <button className={`chip ${kind === 'all' ? 'on' : ''}`} onClick={() => setKind('all')}>
            全部 {failed.length}
          </button>
          {kinds.map((k) => (
            <button key={k} className={`chip ${kind === k ? 'on' : ''}`} onClick={() => setKind(k)}>
              {ERROR_KIND_LABEL[k]} {failed.filter((t) => classifyError(t.error) === k).length}
            </button>
          ))}
        </div>
        {failed.length > 0 && (
          <div className="toolbar-actions">
            <button className="btn secondary small" onClick={retryAll}>
              全部重试
            </button>
            <button className="btn secondary small" onClick={() => void copyAll()}>
              全部复制日志
            </button>
            <button className="btn secondary small" onClick={reportAll}>
              批量上报
            </button>
          </div>
        )}
      </div>

      {list.length === 0 ? (
        <div className="empty-state">
          <Icon name="checkCircle" size={48} />
          <h2>{failed.length === 0 ? '没有失败的任务' : '该类型下没有失败任务'}</h2>
          <p>下载失败的任务会显示在这里，并附带中文原因和处理建议。</p>
        </div>
      ) : (
        <div className="task-list">
          {list.map((t) => (
            <TaskCard key={t.id} task={t} onDetail={setDetail} />
          ))}
        </div>
      )}

      {detail && (
        <DiagnosisModal
          task={detail}
          onClose={() => setDetail(null)}
          onReport={() => {
            setReportTask(detail)
            setDetail(null)
          }}
        />
      )}

      {batchInput && <ReportDialog input={batchInput} onClose={() => setBatchInput(null)} />}

      {reportTask && (
        <ReportDialog
          input={{
            kind: 'download',
            title: reportTask.title,
            url: reportTask.url,
            message: reportTask.error ?? '下载失败',
            detail: reportTask.errorDetail,
            taskId: reportTask.id
          }}
          onClose={() => setReportTask(null)}
        />
      )}
    </div>
  )
}
