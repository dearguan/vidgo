import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactElement, type ReactNode } from 'react'
import type { AppInfo, DownloadTask, Settings, SniffedMedia, UpdateInfo, UpdateProgress } from '../../shared/types'
import type { View } from './utils'
import { api } from './api'

export type TaskAction = 'pause' | 'resume' | 'cancel' | 'remove' | 'openFolder' | 'openFile' | 'redownload'

interface Toast {
  id: number
  text: string
  type: 'ok' | 'err'
}

interface AppContextValue {
  view: View
  setView: (v: View) => void
  tasks: DownloadTask[]
  settings: Settings | null
  appInfo: AppInfo | null
  updateSettings: (patch: Partial<Settings>, silent?: boolean) => Promise<void>
  chooseDir: () => Promise<void>
  sniffedMedia: SniffedMedia[]
  clearSniffedMedia: () => void
  parseRequest: { url: string; token: number } | null
  requestParse: (url: string) => void
  clearParseRequest: () => void
  openBuiltinBrowser: (url?: string) => Promise<void>
  checkBuiltinCookies: (url: string) => Promise<number>
  /** 打开内置浏览器登录，完成后自动重试解析或重新下载 */
  loginFor: (url: string, taskId?: string) => Promise<void>
  taskAction: (action: TaskAction, id: string, deleteFile?: boolean) => void
  updateInfo: UpdateInfo | null
  checkingUpdate: boolean
  installingUpdate: boolean
  updateProgress: UpdateProgress | null
  checkUpdate: (silent?: boolean) => Promise<void>
  installUpdate: () => Promise<void>
  notify: (text: string, type?: 'ok' | 'err') => void
  toasts: Toast[]
}

const AppContext = createContext<AppContextValue | null>(null)

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext)
  if (!ctx) throw new Error('useApp must be used within AppProvider')
  return ctx
}

export function AppProvider({ children }: { children: ReactNode }): ReactElement {
  const [view, setView] = useState<View>('home')
  const [tasks, setTasks] = useState<DownloadTask[]>([])
  const [settings, setSettings] = useState<Settings | null>(null)
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)
  const [sniffedMedia, setSniffedMedia] = useState<SniffedMedia[]>([])
  const [parseRequest, setParseRequest] = useState<{ url: string; token: number } | null>(null)
  const [toasts, setToasts] = useState<Toast[]>([])
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null)
  const [checkingUpdate, setCheckingUpdate] = useState(false)
  const [installingUpdate, setInstallingUpdate] = useState(false)
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null)
  const toastId = useRef(0)
  const parseToken = useRef(0)
  const statusRef = useRef<Record<string, DownloadTask['status']>>({})
  const pendingLogin = useRef<{ url?: string; taskId?: string }>({})

  const notify = useCallback((text: string, type: 'ok' | 'err' = 'ok') => {
    const id = ++toastId.current
    setToasts((list) => [...list.slice(-1), { id, text, type }])
    window.setTimeout(() => setToasts((list) => list.filter((t) => t.id !== id)), 2500)
  }, [])

  const applyTasks = useCallback(
    (list: DownloadTask[]) => {
      const prev = statusRef.current
      for (const t of list) {
        if (t.status === 'COMPLETED' && prev[t.id] && prev[t.id] !== 'COMPLETED') {
          notify(`下载完成：${t.title}`)
        }
        if (t.status === 'FAILED' && prev[t.id] && prev[t.id] !== 'FAILED') {
          notify(t.error || '下载失败', 'err')
        }
      }
      statusRef.current = Object.fromEntries(list.map((t) => [t.id, t.status]))
      setTasks(list)
    },
    [notify]
  )

  useEffect(() => {
    api.getSettings().then(setSettings)
    api.getAppInfo().then(setAppInfo)
    api.getTasks().then(applyTasks)
    const offSniffer = api.onSnifferFound((media) => {
      setSniffedMedia((list) => [
        media,
        ...list.filter((item) => item.url !== media.url || item.type !== media.type)
      ].slice(0, 200))
      const url = media.url.length > 60 ? media.url.slice(0, 60) + '…' : media.url
      notify(`发现${media.type.toUpperCase()}媒体：${url}`, 'ok')
    })
    const offTasks = api.onTasksChanged(applyTasks)
    return () => {
      offSniffer()
      offTasks()
    }
  }, [applyTasks, notify])

  useEffect(() => {
    const theme = settings?.theme ?? 'system'
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const apply = (): void => {
      const dark = theme === 'dark' || (theme === 'system' && media.matches)
      document.documentElement.dataset.theme = dark ? 'dark' : 'light'
    }
    apply()
    media.addEventListener('change', apply)
    return () => media.removeEventListener('change', apply)
  }, [settings?.theme])

  const updateSettings = useCallback(
    async (patch: Partial<Settings>, silent = false) => {
      const next = await api.setSettings(patch)
      setSettings(next)
      if (!silent) notify('设置已保存')
    },
    [notify]
  )

  const chooseDir = useCallback(async () => {
    const dir = await api.chooseDir()
    if (dir) await updateSettings({ saveDir: dir }, true)
  }, [updateSettings])

  const clearSniffedMedia = useCallback(() => setSniffedMedia([]), [])

  const requestParse = useCallback((url: string) => {
    setParseRequest({ url, token: ++parseToken.current })
    setView('home')
  }, [])

  const clearParseRequest = useCallback(() => setParseRequest(null), [])

  const openBuiltinBrowser = useCallback(async (url?: string) => {
    await api.openBuiltinBrowser(url)
  }, [])

  const checkBuiltinCookies = useCallback(async (url: string) => {
    return api.checkBuiltinCookies(url)
  }, [])

  const loginFor = useCallback(async (url: string, taskId?: string) => {
    pendingLogin.current = { url, taskId }
    try {
      await api.openBuiltinBrowser(url)
      notify('已打开内置浏览器，登录后点右上角「完成登录」')
    } catch (e) {
      notify(e instanceof Error ? e.message : String(e), 'err')
    }
  }, [notify])

  // 侧栏「添加」与「完成登录」都在主窗口处理
  useEffect(() => {
    const offEnqueue = api.onSnifferEnqueue?.((media) => {
      requestParse(media.url)
      notify(`已加入下载列表：${media.title || media.url}`)
    })
    const offLogin = api.onBrowserLoginDone?.(() => {
      const pending = pendingLogin.current
      pendingLogin.current = {}
      if (pending.taskId) {
        notify('已保存登录状态，正在重新下载…')
        void api.redownloadTask(pending.taskId).catch((e) => notify(e instanceof Error ? e.message : String(e), 'err'))
      } else if (pending.url) {
        notify('已保存登录状态，正在重新解析…')
        requestParse(pending.url)
      }
    })
    return () => {
      offEnqueue?.()
      offLogin?.()
    }
  }, [requestParse, notify])

  const taskAction = useCallback((action: TaskAction, id: string, deleteFile = false) => {
    const map: Record<TaskAction, (id: string) => Promise<unknown>> = {
      pause: api.pauseTask,
      resume: api.resumeTask,
      cancel: api.cancelTask,
      remove: (tid) => api.removeTask(tid, deleteFile),
      openFolder: api.openFolder,
      openFile: api.openFile,
      redownload: api.redownloadTask
    }
    void map[action](id).catch((e) => notify(e instanceof Error ? e.message : String(e), 'err'))
  }, [notify])

  const checkUpdate = useCallback(
    async (silent = false) => {
      setCheckingUpdate(true)
      try {
        const info = await api.checkUpdate()
        setUpdateInfo(info)
        if (!silent) {
          notify(info.available ? `发现新版本 v${info.latest}` : '当前已是最新版本')
        }
      } catch (e) {
        if (!silent) notify(e instanceof Error ? e.message : String(e), 'err')
      } finally {
        setCheckingUpdate(false)
      }
    },
    [notify]
  )

  const installUpdate = useCallback(async () => {
    if (!updateInfo?.available) return
    if (!updateInfo.assetUrl) {
      notify('缺少更新包地址，请稍后重试', 'err')
      return
    }
    setInstallingUpdate(true)
    setUpdateProgress(null)
    notify('开始下载更新…')
    try {
      await api.installUpdate(updateInfo.assetUrl, updateInfo.sha256, updateInfo.assetSize)
    } catch (e) {
      notify(e instanceof Error ? e.message : String(e), 'err')
    } finally {
      setInstallingUpdate(false)
    }
  }, [updateInfo, notify])

  useEffect(() => {
    const off = api.onUpdateProgress(setUpdateProgress)
    void checkUpdate(true)
    return off
  }, [checkUpdate])

  return (
    <AppContext.Provider
      value={{
        view,
        setView,
        tasks,
        settings,
        appInfo,
        updateSettings,
        chooseDir,
        sniffedMedia,
        clearSniffedMedia,
        parseRequest,
        requestParse,
        clearParseRequest,
        openBuiltinBrowser,
        checkBuiltinCookies,
        loginFor,
        taskAction,
        updateInfo,
        checkingUpdate,
        installingUpdate,
        updateProgress,
        checkUpdate,
        installUpdate,
        notify,
        toasts
      }}
    >
      {children}
    </AppContext.Provider>
  )
}
