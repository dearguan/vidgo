export type TaskStatus =
  | 'WAITING'
  | 'PARSING'
  | 'READY'
  | 'DOWNLOADING'
  | 'MERGING'
  | 'COMPLETED'
  | 'PAUSED'
  | 'FAILED'
  | 'CANCELLED'

export interface FormatOption {
  id: string
  label: string
  kind: 'video' | 'audio'
  selector: string
  ext?: string
  height?: number
  filesize?: number
  audioFormat?: 'mp3' | 'm4a'
}

export interface MediaInfo {
  url: string
  title: string
  thumbnail?: string
  duration?: number
  uploader?: string
  site?: string
  formats: FormatOption[]
}

export type ParseResult = { ok: true; info: MediaInfo } | { ok: false; message: string; detail?: string }

export interface DownloadTask {
  id: string
  url: string
  title: string
  thumbnail?: string
  duration?: number
  formatId: string
  formatLabel: string
  kind: 'video' | 'audio'
  selector: string
  audioFormat?: 'mp3' | 'm4a'
  height?: number
  saveDir: string
  status: TaskStatus
  progress: number
  downloaded: number
  total: number
  speed: number
  eta: number | null
  error?: string
  errorDetail?: string
  filePath?: string
  createdAt: number
  completedAt?: number
}

export type ThemeMode = 'system' | 'light' | 'dark'

export interface SniffedMedia {
  id: string
  url: string
  type: 'mp4' | 'webm' | 'hls' | 'dash' | 'm4a' | 'mp3' | 'unknown'
  title?: string
  timestamp: number
}

export interface Settings {
  saveDir: string
  proxy: string
  /** 读取系统浏览器 Cookie 直接传给 yt-dlp；none 表示不读 */
  cookieBrowser: 'auto' | 'chrome' | 'edge' | 'firefox' | 'brave' | 'safari' | 'vivaldi' | 'opera' | 'none'
  concurrency: number
  theme: ThemeMode
  clipboardWatch: boolean
}

export interface StartDownloadPayload {
  url: string
  title: string
  thumbnail?: string
  duration?: number
  option: FormatOption
  saveDir: string
}

export interface AppInfo {
  version: string
  ytdlp: string
}

export interface Api {
  parse(url: string): Promise<ParseResult>
  cancelParse(): Promise<void>
  startDownload(payload: StartDownloadPayload): Promise<DownloadTask>
  pauseTask(id: string): Promise<void>
  resumeTask(id: string): Promise<void>
  cancelTask(id: string): Promise<void>
  removeTask(id: string, deleteFile?: boolean): Promise<void>
  redownloadTask(id: string): Promise<void>
  openFolder(taskId: string): Promise<void>
  openFile(taskId: string): Promise<void>
  openSaveDir(): Promise<void>
  getTasks(): Promise<DownloadTask[]>
  getSettings(): Promise<Settings>
  setSettings(patch: Partial<Settings>): Promise<Settings>
  getAppInfo(): Promise<AppInfo>
  chooseDir(): Promise<string | null>
  openBuiltinBrowser(): Promise<void>
  checkBuiltinCookies(url: string): Promise<number>
  readClipboard(): Promise<string>
  onTasksChanged(cb: (tasks: DownloadTask[]) => void): () => void
  onSnifferFound(cb: (media: SniffedMedia) => void): () => void
}
