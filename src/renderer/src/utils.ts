import type { DownloadTask } from '../../shared/types'

export type View = 'home' | 'downloading' | 'completed' | 'failed' | 'library' | 'capture' | 'subscriptions' | 'settings'

export const ACTIVE_STATUS: DownloadTask['status'][] = ['WAITING', 'PARSING', 'READY', 'DOWNLOADING', 'MERGING']

export const STATUS_LABEL: Record<DownloadTask['status'], string> = {
  WAITING: '排队中',
  PARSING: '解析中',
  READY: '待下载',
  DOWNLOADING: '下载中',
  MERGING: '合并中',
  COMPLETED: '已完成',
  PAUSED: '已暂停',
  FAILED: '下载失败',
  CANCELLED: '已取消'
}

export function formatBytes(n?: number): string {
  if (!n || n <= 0) return '大小未知'
  const units = ['B', 'KB', 'MB', 'GB']
  let v = n
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`
}

export function formatSpeed(n?: number): string {
  if (!n || n <= 0) return ''
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB/s`
  return `${(n / 1024 / 1024).toFixed(1)} MB/s`
}

export function formatDuration(sec?: number): string {
  if (!sec || sec <= 0) return ''
  const s = Math.floor(sec % 60)
  const m = Math.floor((sec / 60) % 60)
  const h = Math.floor(sec / 3600)
  const pad = (x: number): string => String(x).padStart(2, '0')
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`
}

export function formatEta(sec: number | null): string {
  if (!sec || sec <= 0) return ''
  if (sec < 60) return `剩余 ${Math.ceil(sec)} 秒`
  if (sec < 3600) return `剩余 ${Math.ceil(sec / 60)} 分钟`
  return `剩余 ${Math.floor(sec / 3600)} 小时 ${Math.ceil((sec % 3600) / 60)} 分钟`
}

export function formatTime(ts?: number): string {
  if (!ts) return ''
  const d = new Date(ts)
  const pad = (x: number): string => String(x).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function isHttpUrl(value: string): boolean {
  try {
    const u = new URL(value.trim())
    return u.protocol === 'http:' || u.protocol === 'https:'
  } catch {
    return false
  }
}

export type ErrorKind = 'network' | 'parse' | 'disk' | 'permission' | 'protected' | 'other'

export const ERROR_KIND_LABEL: Record<ErrorKind, string> = {
  network: '网络错误',
  parse: '解析失败',
  disk: '磁盘空间不足',
  permission: '权限受限',
  protected: '内容保护',
  other: '其他错误'
}

export function classifyError(text?: string): ErrorKind {
  const t = text ?? ''
  if (/磁盘|空间不足/.test(t)) return 'disk'
  if (/权限|没有写入/.test(t)) return 'permission'
  if (/受保护|DRM/.test(t)) return 'protected'
  if (/网络|连接|代理|超时/.test(t)) return 'network'
  if (/解析|网站|登录|404|不存在|失效/.test(t)) return 'parse'
  return 'other'
}
