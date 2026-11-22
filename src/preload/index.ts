import { contextBridge, ipcRenderer } from 'electron'
import type { Api, DownloadTask, Settings, StartDownloadPayload } from '../shared/types'

function cleanError(e: unknown): Error {
  const raw = e instanceof Error ? e.message : String(e)
  return new Error(raw.replace(/^Error invoking remote method '[^']+':\s*(Error:\s*)?/, ''))
}

async function invoke<T>(channel: string, ...args: unknown[]): Promise<T> {
  try {
    return (await ipcRenderer.invoke(channel, ...args)) as T
  } catch (e) {
    throw cleanError(e)
  }
}

const api: Api = {
  parse: (url: string) => invoke('media:parse', url),
  cancelParse: () => invoke('media:parseCancel'),
  startDownload: (payload: StartDownloadPayload) => invoke('download:start', payload),
  pauseTask: (id: string) => invoke('task:pause', id),
  resumeTask: (id: string) => invoke('task:resume', id),
  cancelTask: (id: string) => invoke('task:cancel', id),
  removeTask: (id: string, deleteFile?: boolean) => invoke('task:remove', id, deleteFile === true),
  redownloadTask: (id: string) => invoke('task:redownload', id),
  openFolder: (id: string) => invoke('task:openFolder', id),
  openFile: (id: string) => invoke('task:openFile', id),
  openSaveDir: () => invoke('shell:openSaveDir'),
  getTasks: () => invoke('tasks:get'),
  getSettings: () => invoke('settings:get'),
  setSettings: (patch: Partial<Settings>) => invoke('settings:set', patch),
  getAppInfo: () => invoke('app:info'),
  chooseDir: () => invoke('dialog:chooseDir'),
  openBuiltinBrowser: () => invoke('browser:openBuiltin'),
  checkBuiltinCookies: (url: string) => invoke('browser:hasCookies', url),
  readClipboard: () => invoke('clipboard:read'),
  onTasksChanged: (cb: (tasks: DownloadTask[]) => void) => {
    const handler = (_e: Electron.IpcRendererEvent, tasks: DownloadTask[]): void => cb(tasks)
    ipcRenderer.on('tasks:changed', handler)
    return () => ipcRenderer.removeListener('tasks:changed', handler)
  },
  onSnifferFound: (cb: (media: import('../shared/types').SniffedMedia) => void) => {
    const handler = (_e: Electron.IpcRendererEvent, media: import('../shared/types').SniffedMedia): void => cb(media)
    ipcRenderer.on('sniffer:found', handler)
    return () => ipcRenderer.removeListener('sniffer:found', handler)
  }
}

contextBridge.exposeInMainWorld('api', api)
