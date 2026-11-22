import { app, BrowserWindow, clipboard, dialog, ipcMain, shell, nativeTheme } from 'electron'
import path from 'node:path'
import fs from 'node:fs'
import { execFile } from 'node:child_process'
import { Store } from './store'
import { DownloadManager } from './downloader'
import { parseMedia, cancelActiveParse, ParseError } from './ytdlp'
import { ytdlpPath } from './binaries'
import { openBuiltinBrowser, exportCookiesForUrl, isBuiltinBrowserOpen, writeCookiesToFileForUrl } from './builtinBrowser'
import type { AppInfo, DownloadTask, ParseResult, Settings, StartDownloadPayload } from '../shared/types'

let win: BrowserWindow | null = null
let store: Store
let manager: DownloadManager
let ytdlpVersion = ''

function isHttpUrl(value: string): boolean {
  try {
    const u = new URL(value)
    return u.protocol === 'http:' || u.protocol === 'https:'
  } catch {
    return false
  }
}

function broadcastTasks(tasks: DownloadTask[]): void {
  win?.webContents.send('tasks:changed', tasks)
}

function createWindow(): void {
  win = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 980,
    minHeight: 640,
    show: false,
    backgroundColor: nativeTheme.shouldUseDarkColors ? '#171B24' : '#F5F7FC',
    autoHideMenuBar: true,
    title: 'Vidgo',
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  })

  win.once('ready-to-show', () => win?.show())

  const devUrl = process.env['ELECTRON_RENDERER_URL']
  if (devUrl) {
    win.loadURL(devUrl)
  } else {
    win.loadFile(path.join(__dirname, '../renderer/index.html'))
  }

  win.webContents.setWindowOpenHandler(({ url }) => {
    if (isHttpUrl(url)) shell.openExternal(url)
    return { action: 'deny' }
  })

  win.on('closed', () => {
    win = null
  })
}

function registerIpc(): void {
  ipcMain.handle('media:parse', async (_e, url: string): Promise<ParseResult> => {
    if (!isHttpUrl(url)) return { ok: false, message: '请输入有效的视频网址（以 http:// 或 https:// 开头）。' }
    try {
      const cookieFile = await writeCookiesToFileForUrl(url)
      const info = await parseMedia(url.trim(), store.data.settings, cookieFile)
      return { ok: true, info }
    } catch (e) {
      const err = e as ParseError
      return { ok: false, message: err.message || '解析失败，请稍后重试。', detail: err.detail }
    }
  })

  ipcMain.handle('media:parseCancel', () => cancelActiveParse())

  ipcMain.handle('download:start', (_e, payload: StartDownloadPayload) => {
    if (!isHttpUrl(payload.url)) throw new Error('视频地址无效。')
    if (!payload.saveDir) throw new Error('请选择保存位置。')
    return manager.start(payload)
  })

  ipcMain.handle('task:pause', (_e, id: string) => manager.pause(id))
  ipcMain.handle('task:resume', (_e, id: string) => manager.resume(id))
  ipcMain.handle('task:cancel', (_e, id: string) => manager.cancel(id))
  ipcMain.handle('task:remove', (_e, id: string, deleteFile?: boolean) => manager.remove(id, deleteFile === true))
  ipcMain.handle('task:redownload', (_e, id: string) => manager.redownload(id))

  ipcMain.handle('task:openFolder', (_e, id: string) => {
    const folder = manager.openFolder(id)
    if (!folder) return
    const task = manager.find(id)
    if (task?.filePath) shell.showItemInFolder(task.filePath)
    else shell.openPath(folder)
  })

  ipcMain.handle('task:openFile', (_e, id: string) => {
    const task = manager.find(id)
    if (task?.filePath) shell.openPath(task.filePath)
  })

  ipcMain.handle('tasks:get', () => manager.list())

  ipcMain.handle('shell:openSaveDir', () => {
    const dir = store.data.settings.saveDir
    fs.mkdirSync(dir, { recursive: true })
    shell.openPath(dir)
  })

  ipcMain.handle('app:info', async (): Promise<AppInfo> => {
    if (!ytdlpVersion) {
      ytdlpVersion = await new Promise<string>((resolve) => {
        execFile(ytdlpPath(), ['--version'], { timeout: 10_000 }, (err, stdout) => {
          resolve(err ? '未知' : stdout.trim())
        })
      })
    }
    return { version: app.getVersion(), ytdlp: ytdlpVersion }
  })

  ipcMain.handle('settings:get', () => store.data.settings)
  ipcMain.handle('settings:set', (_e, patch: Partial<Settings>) => {
    store.data.settings = { ...store.data.settings, ...patch }
    nativeTheme.themeSource = store.data.settings.theme
    store.save()
    return store.data.settings
  })

  ipcMain.handle('browser:openBuiltin', () => openBuiltinBrowser(win))
  ipcMain.handle('browser:hasCookies', async (_e, url: string) => {
    const txt = await exportCookiesForUrl(url)
    return txt.trim().split('\n').filter((l) => !l.startsWith('#') && l.trim()).length
  })

  ipcMain.handle('dialog:chooseDir', async () => {
    if (!win) return null
    const result = await dialog.showOpenDialog(win, {
      title: '选择视频保存位置',
      properties: ['openDirectory', 'createDirectory']
    })
    return result.canceled ? null : result.filePaths[0]
  })

  ipcMain.handle('clipboard:read', () => clipboard.readText())
}

app.whenReady().then(() => {
  store = new Store()
  nativeTheme.themeSource = store.data.settings.theme
  manager = new DownloadManager(store, broadcastTasks)
  registerIpc()
  manager.init()
  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('before-quit', () => {
  manager?.shutdown()
  store?.flush()
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})
