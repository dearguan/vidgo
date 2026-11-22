import { BrowserWindow, session, app, ipcMain } from 'electron'
import type { Cookie } from 'electron'
import path from 'node:path'
import fs from 'node:fs'

const PERSIST_PARTITION = 'persist:vidgo-browser'
let browserWin: BrowserWindow | null = null

/** 匹配视频/音频/流媒体请求的扩展名和 MIME */
const VIDEO_EXT = [
  '.mp4', '.webm', '.mkv', '.mov', '.avi', '.flv', '.wmv', '.m4v', '.3gp',
  '.m3u8', '.mpd', '.smil', '.vtt', '.srt', '.m4a', '.mp3', '.aac', '.flac', '.opus', '.wav'
]
const VIDEO_MIME_RE = /^(video\/|audio\/|application\/vnd\.apple\.mpegurl|application\/dash\+xml)/i

interface SniffedMedia {
  id: string
  url: string
  type: 'mp4' | 'webm' | 'hls' | 'dash' | 'm4a' | 'mp3' | 'unknown'
  title?: string
  timestamp: number
}

export function getBrowserSession(): Electron.Session {
  return session.fromPartition(PERSIST_PARTITION, { cache: true })
}

function classifyMedia(url: string): SniffedMedia['type'] {
  const lower = url.toLowerCase().split('?')[0]
  if (lower.endsWith('.m3u8')) return 'hls'
  if (lower.endsWith('.mpd')) return 'dash'
  if (lower.endsWith('.mp4')) return 'mp4'
  if (lower.endsWith('.webm')) return 'webm'
  if (lower.endsWith('.m4a')) return 'm4a'
  if (lower.endsWith('.mp3')) return 'mp3'
  if (VIDEO_EXT.some((ext) => lower.endsWith(ext))) return 'unknown'
  return 'unknown'
}

function looksLikeMediaUrl(url: string): boolean {
  // URL 扩展名匹配
  const pathLower = new URL(url).pathname.toLowerCase()
  if (VIDEO_EXT.some((ext) => pathLower.endsWith(ext))) return true
  // 常见 CDN 媒体路径
  if (/video|media|vod|play|stream|hls|dash|audio/i.test(pathLower)) {
    if (/\.(mp4|webm|m3u8|mpd|m4a|mp3|ts)\b/i.test(url)) return true
  }
  return false
}

export function openBuiltinBrowser(mainWin: BrowserWindow | null): void {
  if (browserWin && !browserWin.isDestroyed()) {
    browserWin.focus()
    return
  }
  const sniffed = new Map<string, SniffedMedia>()

  const session = getBrowserSession()

  // MediaSniffer：监听 session 的所有请求
  session.webRequest.onBeforeRequest((details, callback) => {
    const url = details.url
    if (!url.startsWith('http')) return callback({ cancel: false })
    if (!looksLikeMediaUrl(url)) return callback({ cancel: false })

    const media: SniffedMedia = {
      id: `${details.id}-${url.slice(-16)}`,
      url,
      type: classifyMedia(url),
      timestamp: Date.now()
    }
    // 去重：同一 URL 10 秒内只报一次
    const prev = sniffed.get(url)
    if (!prev || Date.now() - prev.timestamp > 10000) {
      sniffed.set(url, media)
      mainWin?.webContents.send('sniffer:found', media)
    }
    return callback({ cancel: false })
  })

  // 额外：通过 webContents 监听响应头里的 Content-Type
  const filter = { urls: ['*://*/*'] }
  session.webRequest.onHeadersReceived(filter, (details, callback) => {
    const ct = (details.responseHeaders?.['content-type'] || details.responseHeaders?.['Content-Type'] || [])[0] || ''
    if (VIDEO_MIME_RE.test(ct) && looksLikeMediaUrl(details.url)) {
      const url = details.url
      const media: SniffedMedia = {
        id: `${details.id}-${url.slice(-16)}`,
        url,
        type: classifyMedia(url),
        timestamp: Date.now()
      }
      const prev = sniffed.get(url)
      if (!prev || Date.now() - prev.timestamp > 10000) {
        sniffed.set(url, media)
        mainWin?.webContents.send('sniffer:found', media)
      }
    }
    return callback({ cancel: false })
  })

  browserWin = new BrowserWindow({
    width: 1100,
    height: 780,
    minWidth: 640,
    minHeight: 480,
    title: 'Vidgo 内置浏览器 — 发现视频自动下载',
    autoHideMenuBar: true,
    backgroundColor: '#ffffff',
    webPreferences: {
      session,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  })

  const devUrl = process.env['ELECTRON_RENDERER_URL']
  if (devUrl) {
    browserWin.loadURL(devUrl.replace(/\/index\.html$/, '/builtin.html'))
  } else {
    browserWin.loadFile(path.join(__dirname, '../renderer/builtin.html'))
  }

  browserWin.on('closed', () => {
    browserWin = null
  })

  // 转发当前页面 URL 到主窗口（可选）
  browserWin.webContents.on('did-navigate', (_e, url) => {
    mainWin?.webContents.send('sniffer:navigate', url)
  })
}

export function isBuiltinBrowserOpen(): boolean {
  return !!(browserWin && !browserWin.isDestroyed())
}

/** 从内置浏览器 session 导出 Netscape 格式的 cookies.txt 字符串 */
export async function exportCookiesForUrl(url: string): Promise<string> {
  const s = getBrowserSession()
  const urlObj = new URL(url)
  const host = urlObj.hostname
  const domainCandidates = new Set<string>([host])
  const dotIndex = host.indexOf('.')
  if (dotIndex >= 0) domainCandidates.add(host.slice(dotIndex + 1))

  const all = await s.cookies.get({})
  const matched = all.filter((c: Cookie) => {
    const d = c.domain || ''
    const cookieDomain = d.startsWith('.') ? d.slice(1) : d
    for (const cand of domainCandidates) {
      if (cookieDomain === cand || cookieDomain.endsWith('.' + cand)) return true
    }
    return false
  })

  if (matched.length === 0) return ''

  const lines = ['# Netscape HTTP Cookie File', '# Auto-generated by Vidgo built-in browser']
  for (const c of matched) {
    const expires = c.expirationDate ? Math.floor(c.expirationDate) : 0
    const rawDomain = c.domain || ''
    const domain = rawDomain.startsWith('.') ? rawDomain : '.' + rawDomain
    const flag = rawDomain.startsWith('.') ? 'TRUE' : 'FALSE'
    lines.push(
      `${domain}\t${flag}\t${c.path || '/'}\t${c.secure ? 'TRUE' : 'FALSE'}\t${expires}\t${c.name}\t${c.value}`
    )
  }
  return lines.join('\n') + '\n'
}

export async function writeCookiesToFileForUrl(url: string): Promise<string | undefined> {
  const txt = await exportCookiesForUrl(url)
  if (!txt) return undefined
  try {
    const tmp = path.join(app.getPath('userData'), 'cookies_builtin.txt')
    fs.writeFileSync(tmp, txt)
    return tmp
  } catch {
    return undefined
  }
}
