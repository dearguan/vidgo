import { useEffect, useRef, useState, type ReactElement, type ReactNode } from 'react'
import { Icon } from '../Icon'

interface ModalProps {
  title: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  width?: number
  persistent?: boolean
}

export function Modal({ title, onClose, children, footer, width = 480, persistent }: ModalProps): ReactElement {
  const boxRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    const first = boxRef.current?.querySelector<HTMLElement>('button, input, [tabindex]')
    first?.focus()
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div
      className="modal-overlay"
      onMouseDown={(e) => {
        if (!persistent && e.target === e.currentTarget) onClose()
      }}
    >
      <div className="modal" style={{ width }} ref={boxRef} role="dialog" aria-label={title}>
        <div className="modal-head">
          <h3>{title}</h3>
          <button className="icon-btn" onClick={onClose} aria-label="关闭" title="关闭">
            <Icon name="close" />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer ? <div className="modal-foot">{footer}</div> : null}
      </div>
    </div>
  )
}

interface ConfirmProps {
  title: string
  body: ReactNode
  confirmText?: string
  onConfirm: (deleteFile: boolean) => void
  onClose: () => void
  withDeleteFile?: boolean
}

export function ConfirmModal({ title, body, confirmText = '删除', onConfirm, onClose, withDeleteFile }: ConfirmProps): ReactElement {
  const [deleteFile, setDeleteFile] = useState(false)
  return (
    <Modal
      title={title}
      width={400}
      onClose={onClose}
      persistent
      footer={
        <>
          <button className="btn secondary" onClick={onClose}>
            取消
          </button>
          <button className="btn danger" onClick={() => onConfirm(deleteFile)}>
            {confirmText}
          </button>
        </>
      }
    >
      <div className="confirm-body">{body}</div>
      {withDeleteFile ? (
        <label className="confirm-check">
          <input type="checkbox" checked={deleteFile} onChange={(e) => setDeleteFile(e.target.checked)} />
          同时删除本地文件
        </label>
      ) : null}
    </Modal>
  )
}
