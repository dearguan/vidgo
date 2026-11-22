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

export interface LibraryFile {
  path: string
  name: string
  extension: string
  size: number
  modifiedAt: number
}

export interface SubscriptionEntry {
  entryId: string
  url: string
  title: string
  thumbnail?: string
  duration?: number
  uploadDate?: string
  firstSeenAt: number
}

export interface Subscription {
  id: string
  url: string
  title: string
  createdAt: number
  lastCheckedAt?: number
  entries: SubscriptionEntry[]
}

export interface SubscriptionCheckResult {
  subscription: Subscription
  newEntryIds: string[]
}

export interface SniffedMedia {
  id: string
  url: string
  type: 'mp4' | 'webm' | 'hls' | 'dash' | 'm4a' | 'mp3' | 'unknown'
  title?: string
  pageUrl?: string
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
  /** 仅保存在本机，用于把问题上报为 GitHub Issue */
  githubToken: string
  reportRepo: string
}

export type ReportKind = 'parse' | 'download' | 'bug'

export interface ReportInput {
  kind: ReportKind
  title: string
  url?: string
  message?: string
  detail?: string
  taskId?: string
  note?: string
  jsErrors?: string[]
}

export interface ReportResult {
  reportId: string
  mode: 'github' | 'local'
  issueUrl?: string
  issueNumber?: number
  savedPath: string
  uploadError?: string
  text: string
}

export interface UpdateInfo {
  available: boolean
  current: string
  latest: string
  notes?: string
  assetUrl?: string
  assetSize?: number
  sha256?: string
  publishedAt?: string
}

export interface UpdateProgress {
  downloaded: number
  total: number
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
  listLibraryFiles?(): Promise<LibraryFile[]>
  openLibraryFile?(path: string): Promise<void>
  openLibraryFolder?(path: string): Promise<void>
  deleteLibraryFile?(path: string): Promise<void>
  openBuiltinBrowser(url?: string): Promise<void>
  navigateBuiltinBrowser?(url: string): Promise<void>
  reloadBuiltinBrowser?(): Promise<void>
  closeBuiltinBrowser?(): Promise<void>
  listSubscriptions?(): Promise<Subscription[]>
  addSubscription?(url: string): Promise<Subscription>
  checkSubscription?(id: string): Promise<SubscriptionCheckResult>
  deleteSubscription?(id: string): Promise<void>
  checkBuiltinCookies(url: string): Promise<number>
  readClipboard(): Promise<string>
  submitReport?(input: ReportInput): Promise<ReportResult>
  checkUpdate?(): Promise<UpdateInfo>
  installUpdate?(assetUrl: string, sha256?: string, size?: number): Promise<void>
  openExternal?(url: string): Promise<void>
  onUpdateProgress?(cb: (progress: UpdateProgress) => void): () => void
  onTasksChanged(cb: (tasks: DownloadTask[]) => void): () => void
  onSnifferFound(cb: (media: SniffedMedia) => void): () => void
}
