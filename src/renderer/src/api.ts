/**
 * 统一 API 层：自动检测运行时环境（Electron / Tauri）
 * - Electron 版走 window.api（preload 注入）
 * - Tauri 版走 @tauri-apps/api invoke + listen
 *
 * 这样 src/renderer/src/ 下所有 React 组件不用改。
 */

import type {
  AppInfo,
  DownloadTask,
  LibraryFile,
  ParseResult,
  ReportInput,
  ReportResult,
  Settings,
  SniffedMedia,
  Subscription,
  SubscriptionCheckResult,
  UpdateInfo,
  UpdateProgress
} from '../../shared/types'

type Listener<T> = (cb: (data: T) => void) => () => void

function isTauri(): boolean {
  return typeof (globalThis as any).__TAURI__ !== 'undefined'
}

function unsupported(feature: string): Promise<never> {
  return Promise.reject(new Error(`当前运行时不支持${feature}`))
}

// ---------- Electron 实现 ----------
const electronApi = {
  parse: (url: string): Promise<ParseResult> =>
    (globalThis as any).window?.api?.parse(url) ?? Promise.reject('no api'),
  cancelParse: (): Promise<void> =>
    (globalThis as any).window?.api?.cancelParse?.() ?? Promise.resolve(),
  startDownload: (payload: any): Promise<DownloadTask> =>
    (globalThis as any).window?.api?.startDownload(payload) ?? Promise.reject('no api'),
  pauseTask: (id: string): Promise<void> =>
    (globalThis as any).window?.api?.pauseTask(id) ?? Promise.resolve(),
  resumeTask: (id: string): Promise<void> =>
    (globalThis as any).window?.api?.resumeTask(id) ?? Promise.resolve(),
  cancelTask: (id: string): Promise<void> =>
    (globalThis as any).window?.api?.cancelTask(id) ?? Promise.resolve(),
  removeTask: (id: string, deleteFile = false): Promise<void> =>
    (globalThis as any).window?.api?.removeTask?.(id, deleteFile) ?? Promise.resolve(),
  redownloadTask: (id: string): Promise<DownloadTask | undefined> =>
    (globalThis as any).window?.api?.redownloadTask?.(id) ?? Promise.resolve(undefined),
  openFolder: (id: string): Promise<void> =>
    (globalThis as any).window?.api?.openFolder?.(id) ?? Promise.resolve(),
  openFile: (id: string): Promise<void> =>
    (globalThis as any).window?.api?.openFile?.(id) ?? Promise.resolve(),
  getTasks: (): Promise<DownloadTask[]> =>
    (globalThis as any).window?.api?.getTasks?.() ?? Promise.resolve([]),
  openSaveDir: (): Promise<void> =>
    (globalThis as any).window?.api?.openSaveDir?.() ?? Promise.resolve(),
  chooseDir: (): Promise<string | null> =>
    (globalThis as any).window?.api?.chooseDir?.() ?? Promise.resolve(null),
  getSettings: (): Promise<Settings> =>
    (globalThis as any).window?.api?.getSettings?.() ?? Promise.resolve({} as Settings),
  setSettings: (patch: Partial<Settings>): Promise<Settings> =>
    (globalThis as any).window?.api?.setSettings?.(patch) ?? Promise.reject('no api'),
  getAppInfo: (): Promise<AppInfo> =>
    (globalThis as any).window?.api?.getAppInfo?.() ?? Promise.resolve({ version: '0.0.0', ytdlp: '' }),
  readClipboard: (): Promise<string> =>
    (globalThis as any).window?.api?.readClipboard?.() ?? Promise.resolve(''),
  listLibraryFiles: (): Promise<LibraryFile[]> =>
    (globalThis as any).window?.api?.listLibraryFiles?.() ?? Promise.resolve([]),
  openLibraryFile: (path: string): Promise<void> =>
    (globalThis as any).window?.api?.openLibraryFile?.(path) ?? unsupported('打开视频库文件'),
  openLibraryFolder: (path: string): Promise<void> =>
    (globalThis as any).window?.api?.openLibraryFolder?.(path) ?? unsupported('打开文件所在目录'),
  deleteLibraryFile: (path: string): Promise<void> =>
    (globalThis as any).window?.api?.deleteLibraryFile?.(path) ?? unsupported('删除视频库文件'),
  openBuiltinBrowser: (url?: string): Promise<void> =>
    (globalThis as any).window?.api?.openBuiltinBrowser?.(url) ?? unsupported('打开内置浏览器'),
  navigateBuiltinBrowser: (url: string): Promise<void> =>
    (globalThis as any).window?.api?.navigateBuiltinBrowser?.(url) ?? unsupported('内置浏览器导航'),
  reloadBuiltinBrowser: (): Promise<void> =>
    (globalThis as any).window?.api?.reloadBuiltinBrowser?.() ?? unsupported('刷新内置浏览器'),
  closeBuiltinBrowser: (): Promise<void> =>
    (globalThis as any).window?.api?.closeBuiltinBrowser?.() ?? unsupported('关闭内置浏览器'),
  listSubscriptions: (): Promise<Subscription[]> =>
    (globalThis as any).window?.api?.listSubscriptions?.() ?? Promise.resolve([]),
  addSubscription: (url: string): Promise<Subscription> =>
    (globalThis as any).window?.api?.addSubscription?.(url) ?? unsupported('添加订阅'),
  checkSubscription: (id: string): Promise<SubscriptionCheckResult> =>
    (globalThis as any).window?.api?.checkSubscription?.(id) ?? unsupported('检查订阅'),
  deleteSubscription: (id: string): Promise<void> =>
    (globalThis as any).window?.api?.deleteSubscription?.(id) ?? unsupported('删除订阅'),
  checkBuiltinCookies: (url: string): Promise<number> =>
    (globalThis as any).window?.api?.checkBuiltinCookies?.(url) ?? Promise.resolve(0),
  submitReport: (_input: ReportInput): Promise<ReportResult> => unsupported('问题上报'),
  checkUpdate: (): Promise<UpdateInfo> => unsupported('检查更新'),
  installUpdate: (_assetUrl?: string, _sha256?: string, _size?: number): Promise<void> =>
    unsupported('自动更新'),
  openExternal: (url: string): Promise<void> => {
    window.open(url, '_blank')
    return Promise.resolve()
  },
  onUpdateProgress: (): (() => void) => () => {},
  onTasksChanged: (cb: (tasks: DownloadTask[]) => void): (() => void) =>
    (globalThis as any).window?.api?.onTasksChanged?.(cb) ?? (() => {}),
  onSnifferFound: (cb: (media: SniffedMedia) => void): (() => void) =>
    (globalThis as any).window?.api?.onSnifferFound?.(cb) ?? (() => {}),
  onTaskProgress: (cb: (ev: any) => void): (() => void) =>
    (globalThis as any).window?.api?.onTaskProgress?.(cb) ?? (() => {}),
}

// ---------- Tauri 实现 ----------
const tauriApi = {
  async invoke<T = any>(cmd: string, args?: Record<string, any>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<T>(cmd, args)
  },
  async listen<T = any>(event: string, cb: (data: T) => void): Promise<() => void> {
    const { listen } = await import('@tauri-apps/api/event')
    const un = await listen<T>(event, (ev) => cb(ev.payload))
    return () => { un() }
  },
  parse: (url: string) => tauriApi.invoke<ParseResult>('parse_media', { url }),
  cancelParse: () => tauriApi.invoke<void>('parse_cancel'),
  startDownload: (payload: any) => tauriApi.invoke<DownloadTask>('start_download', { payload }),
  pauseTask: (id: string) => tauriApi.invoke<void>('pause_task', { id }),
  resumeTask: (id: string) => tauriApi.invoke<void>('resume_task', { id }),
  cancelTask: (id: string) => tauriApi.invoke<void>('cancel_task', { id }),
  removeTask: (id: string, deleteFile = false) => tauriApi.invoke<void>('remove_task', { id, deleteFile }),
  redownloadTask: (id: string) => tauriApi.invoke<DownloadTask>('redownload_task', { id }),
  openFolder: (id: string) => tauriApi.invoke<void>('open_save_dir'), // 简化
  openFile: (_id: string) => Promise.resolve(), // Tauri 版暂不支持
  getTasks: () => tauriApi.invoke<DownloadTask[]>('get_tasks'),
  openSaveDir: () => tauriApi.invoke<void>('open_save_dir'),
  chooseDir: async (): Promise<string | null> => {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const res = await open({ directory: true, multiple: false })
    if (!res) return null
    return typeof res === 'string' ? res : (res as any)
  },
  getSettings: () => tauriApi.invoke<Settings>('get_settings'),
  setSettings: (patch: Partial<Settings>) => tauriApi.invoke<Settings>('set_settings', { newSettings: patch }),
  getAppInfo: () => tauriApi.invoke<AppInfo>('get_app_info'),
  readClipboard: async () => {
    const { readText } = await import('@tauri-apps/plugin-clipboard-manager')
    try {
      return (await readText()) ?? ''
    } catch {
      return ''
    }
  },
  listLibraryFiles: () => tauriApi.invoke<LibraryFile[]>('list_library_files'),
  openLibraryFile: (path: string) => tauriApi.invoke<void>('open_library_file', { path }),
  openLibraryFolder: (path: string) => tauriApi.invoke<void>('open_library_folder', { path }),
  deleteLibraryFile: (path: string) => tauriApi.invoke<void>('delete_library_file', { path }),
  openBuiltinBrowser: (url?: string) => tauriApi.invoke<void>('open_builtin_browser', { url }),
  navigateBuiltinBrowser: (url: string) => tauriApi.invoke<void>('navigate_builtin_browser', { url }),
  reloadBuiltinBrowser: () => tauriApi.invoke<void>('reload_builtin_browser'),
  closeBuiltinBrowser: () => tauriApi.invoke<void>('close_builtin_browser'),
  listSubscriptions: () => tauriApi.invoke<Subscription[]>('list_subscriptions'),
  addSubscription: (url: string) => tauriApi.invoke<Subscription>('add_subscription', { url }),
  checkSubscription: (id: string) => tauriApi.invoke<SubscriptionCheckResult>('check_subscription', { id }),
  deleteSubscription: (id: string) => tauriApi.invoke<void>('delete_subscription', { id }),
  checkBuiltinCookies: (url: string) => tauriApi.invoke<number>('check_builtin_cookies', { url }),
  submitReport: (input: ReportInput) => tauriApi.invoke<ReportResult>('submit_report', { input }),
  checkUpdate: () => tauriApi.invoke<UpdateInfo>('check_update'),
  installUpdate: (assetUrl: string, sha256?: string, size?: number) =>
    tauriApi.invoke<void>('install_update', { assetUrl, sha256, size }),
  openExternal: (url: string) => tauriApi.invoke<void>('open_external_url', { url }),
  onUpdateProgress: (cb: (progress: UpdateProgress) => void): (() => void) => {
    let un: (() => void) | null = null
    tauriApi.listen<UpdateProgress>('update:progress', cb).then((fn) => { un = fn })
    return () => { un?.() }
  },
  onTasksChanged: (cb: (tasks: DownloadTask[]) => void): (() => void) => {
    let un: (() => void) | null = null
    tauriApi.listen<DownloadTask[]>('tasks:changed', cb).then((fn) => { un = fn })
    return () => { un?.() }
  },
  onSnifferFound: (cb: (media: SniffedMedia) => void): (() => void) => {
    let un: (() => void) | null = null
    tauriApi.listen<any>('sniffer:found', (p) => cb({
      id: p.id,
      url: p.url,
      type: (p.media_type ?? p.mediaType ?? 'unknown') as SniffedMedia['type'],
      title: p.title,
      pageUrl: p.page_url ?? p.pageUrl,
      timestamp: p.timestamp,
    })).then((fn) => { un = fn })
    return () => { un?.() }
  },
  onTaskProgress: (cb: (ev: any) => void): (() => void) => {
    let un: (() => void) | null = null
    tauriApi.listen<any>('task:progress', cb).then((fn) => { un = fn })
    return () => { un?.() }
  },
}

// ---------- 统一导出 ----------
export const api = isTauri() ? tauriApi : electronApi
