/**
 * 统一 API 层：自动检测运行时环境（Electron / Tauri）
 * - Electron 版走 window.api（preload 注入）
 * - Tauri 版走 @tauri-apps/api invoke + listen
 *
 * 这样 src/renderer/src/ 下所有 React 组件不用改。
 */

import type { DownloadTask, ParseResult, Settings, AppInfo, SniffedMedia } from '../../shared/types'

type Listener<T> = (cb: (data: T) => void) => () => void

function isTauri(): boolean {
  return typeof (globalThis as any).__TAURI__ !== 'undefined'
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
  openBuiltinBrowser: (): Promise<void> =>
    (globalThis as any).window?.api?.openBuiltinBrowser?.() ?? Promise.resolve(),
  checkBuiltinCookies: (url: string): Promise<number> =>
    (globalThis as any).window?.api?.checkBuiltinCookies?.(url) ?? Promise.resolve(0),
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
    const { dialog } = await import('@tauri-apps/plugin-dialog')
    const res = await dialog.open({ directory: true, multiple: false })
    if (!res) return null
    return typeof res === 'string' ? res : res as any
  },
  getSettings: () => tauriApi.invoke<Settings>('get_settings'),
  setSettings: (patch: Partial<Settings>) => tauriApi.invoke<Settings>('set_settings', { newSettings: patch }),
  getAppInfo: () => tauriApi.invoke<AppInfo>('get_app_info'),
  readClipboard: async () => {
    const { getText } = await import('@tauri-apps/plugin-clipboard-manager')
    try { return await getText() } catch { return '' }
  },
  openBuiltinBrowser: () => tauriApi.invoke<void>('open_builtin_browser'),
  checkBuiltinCookies: (url: string) => tauriApi.invoke<number>('check_builtin_cookies', { url }),
  onTasksChanged: (cb: (tasks: DownloadTask[]) => void): (() => void) => {
    let un: (() => void) | null = null
    tauriApi.listen<DownloadTask[]>('tasks:changed', cb).then((fn) => { un = fn })
    return () => { un?.() }
  },
  onSnifferFound: (cb: (media: SniffedMedia) => void): (() => void) => {
    let un: (() => void) | null = null
    tauriApi.listen<any>('sniffer:found', (p) => cb({
      id: p.id, url: p.url,
      type: (p.media_type ?? 'unknown') as SniffedMedia['type'],
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
