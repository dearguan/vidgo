import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactElement, type ReactNode } from 'react'
import type { AppInfo, DownloadTask, Settings } from '../../shared/types'
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
  openBuiltinBrowser: () => Promise<void>
  checkBuiltinCookies: (url: string) => Promise<number>
  taskAction: (action: TaskAction, id: string, deleteFile?: boolean) => void
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
  const [toasts, setToasts] = useState<Toast[]>([])
  const toastId = useRef(0)
  const statusRef = useRef<Record<string, DownloadTask['status']>>({})

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
    api.onSnifferFound((media) => {
      const url = media.url.length > 60 ? media.url.slice(0, 60) + '…' : media.url
      notify(`📺 发现${media.type.toUpperCase()}视频：${url}`, 'ok')
    })
    return api.onTasksChanged(applyTasks)
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

  const openBuiltinBrowser = useCallback(async () => {
    await api.openBuiltinBrowser()
  }, [])

  const checkBuiltinCookies = useCallback(async (url: string) => {
    return api.checkBuiltinCookies(url)
  }, [])

  const taskAction = useCallback((action: TaskAction, id: string, deleteFile = false) => {
    const map: Record<TaskAction, (id: string) => Promise<void>> = {
      pause: api.pauseTask,
      resume: api.resumeTask,
      cancel: api.cancelTask,
      remove: (tid) => api.removeTask(tid, deleteFile),
      openFolder: api.openFolder,
      openFile: api.openFile,
      redownload: api.redownloadTask
    }
    void map[action](id)
  }, [])

  return (
    <AppContext.Provider
      value={{ view, setView, tasks, settings, appInfo, updateSettings, chooseDir, openBuiltinBrowser, checkBuiltinCookies, taskAction, notify, toasts }}
    >
      {children}
    </AppContext.Provider>
  )
}
