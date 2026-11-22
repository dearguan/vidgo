import { useState, type ReactElement } from 'react'
import type { ReportInput, ReportResult } from '../../../shared/types'
import { api } from '../api'
import { Icon } from '../Icon'
import { recentErrors } from '../reportErrors'
import { useApp } from '../state'
import { Modal } from './Modal'

export function ReportDialog({
  input,
  onClose
}: {
  input: ReportInput
  onClose: () => void
}): ReactElement {
  const { notify, setView } = useApp()
  const [note, setNote] = useState('')
  const [sending, setSending] = useState(false)
  const [result, setResult] = useState<ReportResult | null>(null)
  const [error, setError] = useState('')

  const submit = async (): Promise<void> => {
    setSending(true)
    setError('')
    try {
      const res = await api.submitReport({ ...input, note, jsErrors: recentErrors() })
      setResult(res)
      if (res.mode === 'github') {
        notify(`已创建 Issue #${res.issueNumber ?? ''}`)
      } else if (res.uploadError) {
        notify('报告已保存到本机，但上传 GitHub 失败', 'err')
      } else {
        notify('报告已保存到本机')
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSending(false)
    }
  }

  const copy = async (text: string): Promise<void> => {
    await navigator.clipboard.writeText(text)
    notify('内容已复制')
  }

  if (result) {
    const ok = result.mode === 'github'
    return (
      <Modal
        title={ok ? '已提交给开发者' : '报告已保存'}
        width={520}
        onClose={onClose}
        footer={
          <>
            {ok && result.issueUrl && (
              <button className="btn secondary" onClick={() => void api.openExternal(result.issueUrl!)}>
                打开 Issue
              </button>
            )}
            <button className="btn secondary" onClick={() => void copy(result.text)}>
              复制报告
            </button>
            <button className="btn primary" onClick={onClose}>
              完成
            </button>
          </>
        }
      >
        <div className="report-result">
          <Icon name={ok ? 'checkCircle' : 'info'} size={28} />
          <div>
            <h4>{ok ? `已创建 Issue #${result.issueNumber ?? ''}` : '已保存到本机，等待发送'}</h4>
            <p className="muted-text">上报编号：{result.reportId}</p>
            <p className="report-path" title={result.savedPath}>
              {result.savedPath}
            </p>
          </div>
        </div>
        {result.uploadError && <p className="field-error">上传失败：{result.uploadError}</p>}
        {!ok && !result.uploadError && (
          <div className="report-hint">
            <p>未配置 GitHub Token，报告只保存在本机。</p>
            <button
              className="btn-text"
              onClick={() => {
                setView('settings')
                onClose()
              }}
            >
              去设置 Token，下次可自动创建 Issue
            </button>
          </div>
        )}
      </Modal>
    )
  }

  return (
    <Modal
      title="上报问题"
      width={560}
      onClose={onClose}
      footer={
        <>
          <button className="btn secondary" onClick={onClose} disabled={sending}>
            取消
          </button>
          <button className="btn primary" onClick={() => void submit()} disabled={sending}>
            {sending ? '正在上报…' : '发送报告'}
          </button>
        </>
      }
    >
      <div className="report-summary">
        <div>
          <span className="panel-label">问题</span>
          <p>{input.message || input.title}</p>
        </div>
        {input.url && (
          <div>
            <span className="panel-label">链接</span>
            <p className="report-path" title={input.url}>
              {input.url}
            </p>
          </div>
        )}
      </div>
      <label className="report-note">
        <span className="panel-label">补充说明（可选）</span>
        <textarea
          className="textarea"
          rows={4}
          value={note}
          placeholder="例如：这个链接昨天还能下载，今天开始报错。"
          onChange={(e) => setNote(e.target.value)}
        />
      </label>
      <p className="muted-text report-privacy">
        将附带：软件版本、系统信息、错误详情、最近失败任务和前端异常。不会包含下载内容或账号密码。
      </p>
      {error && <p className="field-error">{error}</p>}
    </Modal>
  )
}
