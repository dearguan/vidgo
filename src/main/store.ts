import { app } from 'electron'
import path from 'node:path'
import fs from 'node:fs'
import type { DownloadTask, Settings } from '../shared/types'

interface StoreData {
  settings: Settings
  tasks: DownloadTask[]
}

export class Store {
  private file: string
  private timer: NodeJS.Timeout | null = null
  data: StoreData

  constructor() {
    this.file = path.join(app.getPath('userData'), 'videogo.json')
    this.data = {
      settings: {
        saveDir: path.join(app.getPath('downloads'), 'Vidgo'),
        proxy: '',
        cookieBrowser: 'auto',
        concurrency: 3,
        theme: 'system',
        clipboardWatch: true
      },
      tasks: []
    }
    this.load()
  }

  private load(): void {
    // 旧版 Videogo（原 appName "videogo"）自动迁移
    const legacyPath = path.join(path.dirname(app.getPath('userData')), 'videogo', 'videogo.json')
    if (!fs.existsSync(this.file) && fs.existsSync(legacyPath)) {
      try {
        const legacy = JSON.parse(fs.readFileSync(legacyPath, 'utf-8')) as Partial<StoreData>
        if (legacy.settings) this.data.settings = { ...this.data.settings, ...legacy.settings }
        if (Array.isArray(legacy.tasks)) this.data.tasks = legacy.tasks
      } catch {
        // 忽略旧数据损坏
      }
    }
    try {
      if (fs.existsSync(this.file)) {
        const parsed = JSON.parse(fs.readFileSync(this.file, 'utf-8')) as Partial<StoreData>
        if (parsed.settings) this.data.settings = { ...this.data.settings, ...parsed.settings }
        if (Array.isArray(parsed.tasks)) this.data.tasks = parsed.tasks
      }
    } catch {
      // 配置损坏时使用默认值，不阻断启动
    }
  }

  save(): void {
    if (this.timer) return
    this.timer = setTimeout(() => {
      this.timer = null
      this.write()
    }, 300)
  }

  flush(): void {
    if (this.timer) {
      clearTimeout(this.timer)
      this.timer = null
    }
    this.write()
  }

  private write(): void {
    try {
      fs.mkdirSync(path.dirname(this.file), { recursive: true })
      fs.writeFileSync(this.file, JSON.stringify(this.data, null, 2))
    } catch {
      // 忽略写盘失败
    }
  }
}
