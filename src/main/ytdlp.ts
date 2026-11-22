import { spawn, type ChildProcess } from 'node:child_process'
import path from 'node:path'
import { ytdlpPath, ffmpegDir } from './binaries'
import type { FormatOption, MediaInfo, Settings } from '../shared/types'

export class ParseError extends Error {
  detail?: string
  constructor(message: string, detail?: string) {
    super(message)
    this.detail = detail
  }
}

let activeParse: ChildProcess | null = null

export function cancelActiveParse(): void {
  activeParse?.kill()
  activeParse = null
}

interface YtFormat {
  format_id?: string
  ext?: string
  height?: number
  width?: number
  fps?: number
  vcodec?: string
  acodec?: string
  filesize?: number
  filesize_approx?: number
  tbr?: number
}

function formatSize(bytes?: number): number | undefined {
  if (!bytes || bytes <= 0) return undefined
  return Math.round(bytes)
}

function humanError(raw: string): { message: string; detail?: string } {
  const detail = raw.trim().slice(-4000)
  const rules: Array<{ test: RegExp; message: string }> = [
    {
      test: /Unsupported URL|no suitable info extractor|is not a valid URL/i,
      message: '当前网站暂时无法解析。可能是网站更新导致解析器失效，请更新解析引擎后重试。'
    },
    {
      test: /Sign in to confirm|login required|requires authentication|Private video|members-only|Fresh cookies.*needed|cookies are required|cookies must be/i,
      message: '该视频需要 Cookie 才能解析，请在「设置 → 网络与代理」中配置 Cookie 文件后重试。'
    },
    {
      test: /HTTP Error 403|Forbidden/i,
      message: '网站拒绝访问（403）。可能是地区限制或需要登录，请尝试配置代理。'
    },
    {
      test: /HTTP Error 404|not found/i,
      message: '视频不存在或已被删除（404）。'
    },
    {
      test: /getaddrinfo|timed out|Connection refused|Temporary failure in name resolution|SSL/i,
      message: '网络连接失败，请检查网络或代理设置。'
    },
    {
      test: /Requested format is not available/i,
      message: '所选清晰度当前不可用，请重新解析后选择其他清晰度。'
    },
    {
      test: /ffmpeg/i,
      message: 'FFmpeg 处理失败，请检查下载引擎组件是否完整。'
    },
    {
      test: /No space left|ENOSPC|disk quota/i,
      message: '磁盘空间不足，请更换保存位置。'
    },
    {
      test: /Permission denied|EACCES|EPERM/i,
      message: '没有写入权限，请更换保存位置。'
    }
  ]
  const hit = rules.find((r) => r.test.test(detail))
  return { message: hit ? hit.message : '任务失败，展开详情可查看原始错误信息。', detail }
}

export function friendlyError(raw: string): { message: string; detail?: string } {
  return humanError(raw)
}

function baseArgs(settings: Settings, cookieFile?: string): string[] {
  const args = ['--no-playlist', '--no-warnings', '--socket-timeout', '20']
  if (settings.proxy.trim()) args.push('--proxy', settings.proxy.trim())
  // 优先级：内置浏览器 Cookie 文件 > 系统浏览器 Cookie > 无 Cookie
  if (cookieFile) {
    args.push('--cookies', cookieFile)
  } else if (settings.cookieBrowser && settings.cookieBrowser !== 'none') {
    args.push('--cookies-from-browser', settings.cookieBrowser)
  }
  return args
}

export async function parseMedia(url: string, settings: Settings, cookieFile?: string): Promise<MediaInfo> {
  const args = ['-J', ...baseArgs(settings, cookieFile), url]
  const raw = await new Promise<string>((resolve, reject) => {
    let out = ''
    let err = ''
    let done = false
    activeParse?.kill()
    const child = spawn(ytdlpPath(), args, { windowsHide: true })
    activeParse = child
    const cleanup = (): void => {
      if (activeParse === child) activeParse = null
    }
    const timer = setTimeout(() => {
      if (!done) {
        done = true
        child.kill()
        cleanup()
        reject(new ParseError('解析超时，请检查网络后重试。'))
      }
    }, 90_000)

    child.stdout.on('data', (d) => (out += d.toString()))
    child.stderr.on('data', (d) => (err += d.toString()))
    child.on('error', (e) => {
      if (done) return
      done = true
      clearTimeout(timer)
      cleanup()
      reject(new ParseError(`无法启动下载引擎：${e.message}`))
    })
    child.on('close', (code) => {
      if (done) return
      done = true
      clearTimeout(timer)
      cleanup()
      if (code === 0 && out.trim()) resolve(out)
      else {
        const mapped = humanError(err)
        reject(new ParseError(mapped.message, mapped.detail))
      }
    })
  })

  let info: {
    id?: string
    title?: string
    thumbnail?: string
    duration?: number
    uploader?: string
    extractor_key?: string
    extractor?: string
    formats?: YtFormat[]
  }
  try {
    info = JSON.parse(raw)
  } catch {
    throw new ParseError('解析结果异常，请稍后重试。')
  }

  const videos = (info.formats ?? []).filter(
    (f) => f.vcodec && f.vcodec !== 'none' && (f.height ?? 0) > 0
  )
  const audios = (info.formats ?? []).filter((f) => f.acodec && f.acodec !== 'none' && f.vcodec === 'none')
  const audioSize = Math.max(0, ...audios.map((a) => a.filesize ?? a.filesize_approx ?? 0))

  const heights = Array.from(new Set(videos.map((v) => v.height as number))).sort((a, b) => b - a)

  const formats: FormatOption[] = [
    { id: 'best', label: '最佳画质（自动）', kind: 'video', selector: 'bv*+ba/b', ext: 'mp4' }
  ]

  for (const h of heights) {
    const candidates = videos.filter((v) => (v.height as number) <= h)
    const best = candidates.reduce<YtFormat | null>((acc, v) => {
      const size = v.filesize ?? v.filesize_approx ?? 0
      const accSize = acc ? acc.filesize ?? acc.filesize_approx ?? 0 : -1
      return size > accSize ? v : acc
    }, null)
    const videoSize = best?.filesize ?? best?.filesize_approx ?? 0
    const total = videoSize + audioSize
    formats.push({
      id: `h${h}`,
      label: h >= 2160 ? `${h}P · 4K` : `${h}P`,
      kind: 'video',
      selector: `bv*[height<=${h}]+ba/b[height<=${h}]`,
      height: h,
      filesize: formatSize(total)
    })
  }

  formats.push({ id: 'worst', label: '最低可用画质', kind: 'video', selector: 'wv*+wa/w', ext: 'mp4' })
  formats.push({ id: 'audio-m4a', label: '仅音频 · M4A', kind: 'audio', selector: 'ba[ext=m4a]/ba', audioFormat: 'm4a' })
  formats.push({ id: 'audio-mp3', label: '仅音频 · MP3', kind: 'audio', selector: 'ba/b', audioFormat: 'mp3' })

  return {
    url,
    title: info.title?.trim() || '未命名视频',
    thumbnail: info.thumbnail,
    duration: info.duration,
    uploader: info.uploader,
    site: info.extractor_key || info.extractor,
    formats
  }
}

function safeFileName(title: string): string {
  const cleaned = title.replace(/[\\/:*?"<>|\r\n]+/g, '_').trim()
  return (cleaned || 'video').slice(0, 120)
}

export function buildDownloadArgs(
  url: string,
  title: string,
  selector: string,
  saveDir: string,
  settings: Settings,
  audioFormat?: 'mp3' | 'm4a',
  height?: number,
  cookieFile?: string
): string[] {
  const suffix = !audioFormat && height ? ` [${height}P]` : ''
  const outTemplate = path.join(saveDir, `${safeFileName(title)}${suffix}.%(ext)s`)
  const args = [
    '--newline',
    '--no-simulate',
    '--no-quiet',
    '--continue',
    ...baseArgs(settings, cookieFile),
    '--progress-template',
    'download:VGP|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(progress.percent)s',
    '--print',
    'after_move:VGFILE|%(filepath)s',
    '-f',
    selector,
    '-o',
    outTemplate,
    '--ffmpeg-location',
    ffmpegDir()
  ]
  if (audioFormat) {
    args.push('--extract-audio', '--audio-format', audioFormat, '--audio-quality', '0')
  }
  args.push(url)
  return args
}
