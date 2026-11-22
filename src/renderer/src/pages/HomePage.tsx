import { useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from 'react'
import type { FormatOption, MediaInfo } from '../../../shared/types'
import { Icon, Logo } from '../Icon'
import { useApp } from '../state'
import { api } from '../api'
import { formatBytes, formatDuration, isHttpUrl } from '../utils'
import { Thumb } from '../components/TaskCard'
import { ReportDialog } from '../components/ReportDialog'

function middleEllipsis(text: string, max = 46): string {
  if (text.length <= max) return text
  const head = Math.ceil(max * 0.45)
  const tail = Math.floor(max * 0.45)
  return `${text.slice(0, head)}…${text.slice(-tail)}`
}

export function HomePage(): ReactElement {
  const { settings, chooseDir, notify, setView, parseRequest, clearParseRequest } = useApp()
  const [url, setUrl] = useState('')
  const [media, setMedia] = useState<MediaInfo | null>(null)
  const [formatId, setFormatId] = useState('')
  const [audioId, setAudioId] = useState('')
  const [audioOnly, setAudioOnly] = useState(false)
  const [parsing, setParsing] = useState(false)
  const [slowParse, setSlowParse] = useState(false)
  const [parseError, setParseError] = useState<{ message: string; detail?: string } | null>(null)
  const [showDetail, setShowDetail] = useState(false)
  const [clipHint, setClipHint] = useState('')
  const [urlInvalid, setUrlInvalid] = useState(false)
  const [reportOpen, setReportOpen] = useState(false)
  const seq = useRef(0)

  const valid = isHttpUrl(url)

  useEffect(() => {
    if (!parsing) return
    setSlowParse(false)
    const timer = window.setTimeout(() => setSlowParse(true), 10_000)
    return () => window.clearTimeout(timer)
  }, [parsing])

  useEffect(() => {
    if (!settings?.clipboardWatch) return
    let timer: number | undefined
    const check = async (): Promise<void> => {
      const text = (await api.readClipboard()).trim()
      if (isHttpUrl(text)) {
        setClipHint(text)
        window.clearTimeout(timer)
        timer = window.setTimeout(() => setClipHint(''), 8000)
      }
    }
    void check()
    window.addEventListener('focus', check)
    return () => {
      window.removeEventListener('focus', check)
      window.clearTimeout(timer)
    }
  }, [settings?.clipboardWatch])

  const startParse = useCallback(async (requestedUrl?: string) => {
    const target = (requestedUrl ?? url).trim()
    if (requestedUrl !== undefined) setUrl(target)
    if (!isHttpUrl(target)) {
      setUrlInvalid(true)
      return
    }
    setUrlInvalid(false)
    const id = ++seq.current
    setParsing(true)
    setMedia(null)
    setParseError(null)
    setShowDetail(false)
    try {
      const result = await api.parse(target)
      if (id !== seq.current) return
      if (result.ok) {
        setMedia(result.info)
        setFormatId(result.info.formats.find((f) => f.kind === 'video')?.id ?? '')
        setAudioId(result.info.formats.find((f) => f.kind === 'audio')?.id ?? '')
        setAudioOnly(false)
      } else if (result.message !== '已取消解析') {
        setParseError({ message: result.message, detail: result.detail })
      }
    } catch (e) {
      if (id === seq.current) {
        setParseError({ message: e instanceof Error ? e.message : String(e) })
      }
    } finally {
      if (id === seq.current) setParsing(false)
    }
  }, [url])

  useEffect(() => {
    if (!parseRequest) return
    setClipHint('')
    const target = parseRequest.url
    clearParseRequest()
    void startParse(target)
  }, [parseRequest, startParse, clearParseRequest])

  const cancelParse = (): void => {
    seq.current++
    void api.cancelParse()
    setParsing(false)
  }

  const paste = async (): Promise<void> => {
    const text = (await api.readClipboard()).trim()
    if (!text) {
      notify('剪贴板里没有可用的链接', 'err')
      return
    }
    setUrl(text)
    setUrlInvalid(false)
    setClipHint('')
  }

  const videoFormats = useMemo(() => media?.formats.filter((f) => f.kind === 'video') ?? [], [media])
  const audioFormats = useMemo(() => media?.formats.filter((f) => f.kind === 'audio') ?? [], [media])
  const options = audioOnly ? audioFormats : videoFormats
  const selectedId = audioOnly ? audioId : formatId
  const selected: FormatOption | undefined = options.find((f) => f.id === selectedId)

  const startDownload = async (): Promise<void> => {
    if (!media || !selected || !settings) return
    try {
      await api.startDownload({
        url: media.url,
        title: media.title,
        thumbnail: media.thumbnail,
        duration: media.duration,
        option: selected,
        saveDir: settings.saveDir
      })
      notify('已加入下载队列')
      setMedia(null)
      setUrl('')
    } catch (e) {
      notify((e as Error).message, 'err')
    }
  }

  return (
    <div className="home">
      {!media && !parsing && (
        <div className="home-hero">
          <Logo size={56} />
          <h1>粘贴视频链接开始下载</h1>
          <p>粘贴视频网址，解析后选择画质并开始下载。</p>
        </div>
      )}

      <div className={`url-card ${urlInvalid ? 'invalid' : ''}`}>
        <div className="url-input">
          <Icon name="link" size={20} />
          <input
            value={url}
            placeholder="粘贴视频网址，例如 https://…"
            aria-label="视频网址"
            onChange={(e) => {
              setUrl(e.target.value)
              setUrlInvalid(false)
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void startParse()
            }}
          />
          {url && (
            <button className="icon-btn" title="清空" aria-label="清空" onClick={() => setUrl('')}>
              <Icon name="close" size={14} />
            </button>
          )}
        </div>
        <button className="btn secondary" onClick={paste}>
          <Icon name="clipboard" size={16} />
          粘贴
        </button>
        <button className="btn primary" disabled={!valid || parsing} onClick={() => void startParse()}>
          {parsing ? '解析中…' : '开始解析'}
        </button>
      </div>
      {urlInvalid && <p className="field-error">请输入有效的视频网址，需以 http:// 或 https:// 开头。</p>}

      <div className="quick-links">
        <button className="btn-text" onClick={paste}>
          <Icon name="clipboard" size={15} />
          从剪贴板粘贴
        </button>
        <button className="btn-text" onClick={() => setView('capture')}>
          <Icon name="globe" size={15} />
          打开网页捕获
        </button>
        <button className="btn-text" disabled title="后续版本提供">
          <Icon name="list" size={15} />
          导入批量网址
        </button>
      </div>

      {clipHint && !media && (
        <div className="clip-hint">
          <Icon name="clipboard" size={15} />
          <span className="clip-text" title={clipHint}>
            检测到剪贴板中的链接：{middleEllipsis(clipHint, 56)}
          </span>
          <button
            className="btn-text"
            onClick={() => {
              setUrl(clipHint)
              setClipHint('')
            }}
          >
            填入
          </button>
          <button className="btn-text muted" onClick={() => setClipHint('')}>
            忽略
          </button>
        </div>
      )}

      {parsing && (
        <div className="card parse-card">
          <div className="parse-status">
            <Icon name="spinner" className="spin" size={18} />
            <span>正在解析视频信息…</span>
            {slowParse && <span className="muted-text">耗时较长，可能是网络较慢或网站响应缓慢</span>}
            <button className="btn-text" onClick={cancelParse}>
              取消
            </button>
          </div>
          <div className="parse-skeleton">
            <div className="skeleton-thumb" />
            <div className="skeleton-lines">
              <div className="skeleton-line w60" />
              <div className="skeleton-line w35" />
              <div className="skeleton-formats">
                <div className="skeleton-chip" />
                <div className="skeleton-chip" />
                <div className="skeleton-chip" />
              </div>
            </div>
          </div>
        </div>
      )}

      {!parsing && media && (
        <div className="card media-card">
          <div className="media-top">
            <div className="media-cover">
              <Thumb src={media.thumbnail} size="large" />
              {media.duration ? <span className="duration">{formatDuration(media.duration)}</span> : null}
            </div>
            <div className="media-heading">
              <h2 title={media.title}>{media.title}</h2>
              <div className="media-meta">
                {media.site && <span>{media.site}</span>}
                {media.uploader && <span>{media.uploader}</span>}
                {media.duration ? <span>{formatDuration(media.duration)}</span> : null}
              </div>
            </div>
          </div>

          <div className="format-panel">
            <div className="format-panel-head">
              <span className="panel-label">{audioOnly ? '音频格式' : '清晰度'}</span>
              <label className="switch-row">
                <span>仅下载音频</span>
                <button
                  role="switch"
                  aria-checked={audioOnly}
                  aria-label="仅下载音频"
                  className={`switch ${audioOnly ? 'on' : ''}`}
                  onClick={() => setAudioOnly((v) => !v)}
                >
                  <span className="switch-dot" />
                </button>
              </label>
            </div>
            {options.length <= 6 ? (
              <div className="segment-group" role="radiogroup" aria-label={audioOnly ? '音频格式' : '清晰度'}>
                {options.map((f) => (
                  <button
                    key={f.id}
                    role="radio"
                    aria-checked={selectedId === f.id}
                    className={`segment ${selectedId === f.id ? 'selected' : ''}`}
                    onClick={() => (audioOnly ? setAudioId(f.id) : setFormatId(f.id))}
                  >
                    <span className="segment-label">{f.label}</span>
                    <span className="segment-sub">{f.kind === 'audio' ? (f.audioFormat?.toUpperCase() ?? '') : formatBytes(f.filesize)}</span>
                  </button>
                ))}
              </div>
            ) : (
              <select
                className="select"
                value={selectedId}
                onChange={(e) => (audioOnly ? setAudioId(e.target.value) : setFormatId(e.target.value))}
                aria-label="清晰度"
              >
                {options.map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.label}
                    {f.filesize ? `（约 ${formatBytes(f.filesize)}）` : ''}
                  </option>
                ))}
              </select>
            )}
            <div className="format-foot">
              <span className="container-tag">{audioOnly ? (selected?.audioFormat?.toUpperCase() ?? 'M4A') : 'MP4'}</span>
              {selected?.filesize ? <span className="muted-text">预估大小 {formatBytes(selected.filesize)}</span> : null}
            </div>
          </div>

          <div className="save-row">
            <span className="panel-label">保存到</span>
            <span className="save-path" title={settings?.saveDir}>
              {middleEllipsis(settings?.saveDir ?? '')}
            </span>
            <button className="btn secondary small" onClick={() => void chooseDir()}>
              更改
            </button>
          </div>

          <button className="btn primary big" onClick={() => void startDownload()} disabled={!selected}>
            <Icon name="download" size={18} />
            开始下载
          </button>
        </div>
      )}

      {!parsing && parseError && (
        <div className="card error-card">
          <div className="error-card-head">
            <Icon name="alert" size={20} />
            <div>
              <h3>无法解析这个视频</h3>
              <p>{parseError.message}</p>
            </div>
          </div>
          <p className="error-suggestion">建议：检查网址是否完整、网络是否可访问该网站，然后重试。若持续失败，可能是网站更新导致解析器暂时失效。</p>
          <div className="error-actions">
            <button className="btn primary" onClick={() => void startParse()}>
              重试
            </button>
            <button
              className="btn secondary"
              onClick={async () => {
                await navigator.clipboard.writeText(`${parseError.message}\n${parseError.detail ?? ''}`)
                notify('技术详情已复制')
              }}
            >
              <Icon name="copy" size={15} />
              复制技术详情
            </button>
            <button className="btn secondary" onClick={() => setReportOpen(true)}>
              <Icon name="info" size={15} />
              上报给开发者
            </button>
            {parseError.detail && (
              <button className="btn-text" onClick={() => setShowDetail((v) => !v)}>
                {showDetail ? '收起详情' : '展开详情'}
                <Icon name={showDetail ? 'chevronUp' : 'chevronDown'} size={14} />
              </button>
            )}
          </div>
          {showDetail && parseError.detail && <pre className="tech-detail">{parseError.detail}</pre>}
        </div>
      )}

      {!media && !parsing && !parseError && (
        <div className="home-capabilities">支持常见视频网站 · 高清画质 · 音频提取</div>
      )}

      {parseError && (
        <div className="home-capabilities">
          <button className="btn-text muted" onClick={() => setView('settings')}>
            检查网络与代理设置
            <Icon name="chevronRight" size={14} />
          </button>
        </div>
      )}

      {reportOpen && parseError && (
        <ReportDialog
          input={{
            kind: 'parse',
            title: `解析失败：${url}`,
            url,
            message: parseError.message,
            detail: parseError.detail
          }}
          onClose={() => setReportOpen(false)}
        />
      )}
    </div>
  )
}
