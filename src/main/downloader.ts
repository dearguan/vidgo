import { spawn, type ChildProcess } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import crypto from 'node:crypto'
import { buildDownloadArgs, friendlyError } from './ytdlp'
import { ytdlpPath } from './binaries'
import { writeCookiesToFileForUrl } from './builtinBrowser'
import type { DownloadTask, StartDownloadPayload } from '../shared/types'
import type { Store } from './store'

export class DownloadManager {
  private procs = new Map<string, ChildProcess>()
  private pauseRequested = new Set<string>()
  private cancelRequested = new Set<string>()
  private shuttingDown = false

  constructor(
    private store: Store,
    private onChange: (tasks: DownloadTask[]) => void
  ) {}

  init(): void {
    for (const t of this.store.data.tasks) {
      if (t.status === 'DOWNLOADING' || t.status === 'MERGING' || t.status === 'PARSING') {
        t.status = 'PAUSED'
        t.speed = 0
        t.eta = null
      }
    }
    this.persist()
    this.pump()
  }

  list(): DownloadTask[] {
    return this.store.data.tasks
  }

  find(id: string): DownloadTask | undefined {
    return this.store.data.tasks.find((t) => t.id === id)
  }

  start(payload: StartDownloadPayload): DownloadTask {
    const task: DownloadTask = {
      id: crypto.randomUUID(),
      url: payload.url,
      title: payload.title,
      thumbnail: payload.thumbnail,
      duration: payload.duration,
      formatId: payload.option.id,
      formatLabel: payload.option.label,
      kind: payload.option.kind,
      selector: payload.option.selector,
      audioFormat: payload.option.audioFormat,
      height: payload.option.height,
      saveDir: payload.saveDir,
      status: 'WAITING',
      progress: 0,
      downloaded: 0,
      total: 0,
      speed: 0,
      eta: null,
      createdAt: Date.now()
    }
    this.store.data.tasks.unshift(task)
    this.persist()
    this.emit()
    this.pump()
    return task
  }

  pause(id: string): void {
    const task = this.find(id)
    if (!task) return
    const proc = this.procs.get(id)
    if (proc) {
      this.pauseRequested.add(id)
      proc.kill()
    } else {
      task.status = 'PAUSED'
      this.persist()
      this.emit()
    }
  }

  resume(id: string): void {
    const task = this.find(id)
    if (!task) return
    if (task.status !== 'PAUSED' && task.status !== 'FAILED' && task.status !== 'CANCELLED') return
    task.status = 'WAITING'
    task.error = undefined
    task.errorDetail = undefined
    this.cancelRequested.delete(id)
    this.persist()
    this.emit()
    this.pump()
  }

  cancel(id: string): void {
    const task = this.find(id)
    if (!task) return
    const proc = this.procs.get(id)
    if (proc) {
      this.cancelRequested.add(id)
      proc.kill()
    } else {
      task.status = 'CANCELLED'
      this.persist()
      this.emit()
    }
  }

  remove(id: string, deleteFile = false): void {
    const task = this.find(id)
    if (deleteFile && task?.filePath) {
      try {
        fs.unlinkSync(task.filePath)
      } catch {
        // 文件可能已被移动或删除，仅移除记录
      }
    }
    const proc = this.procs.get(id)
    if (proc) {
      this.cancelRequested.add(id)
      proc.kill()
    }
    this.store.data.tasks = this.store.data.tasks.filter((t) => t.id !== id)
    this.persist()
    this.emit()
  }

  redownload(id: string): void {
    const task = this.find(id)
    if (!task) return
    this.start({
      url: task.url,
      title: task.title,
      thumbnail: task.thumbnail,
      duration: task.duration,
      saveDir: task.saveDir,
      option: {
        id: task.formatId,
        label: task.formatLabel,
        kind: task.kind,
        selector: task.selector,
        audioFormat: task.audioFormat,
        height: task.height
      }
    })
  }

  private emit(): void {
    this.onChange(this.list())
  }

  private persist(): void {
    if (this.store.data.tasks.length > 200) {
      this.store.data.tasks = this.store.data.tasks.slice(0, 200)
    }
    this.store.save()
  }

  private async pump(): Promise<void> {
    if (this.shuttingDown) return
    const concurrency = Math.max(1, this.store.data.settings.concurrency || 3)
    while (this.procs.size < concurrency) {
      const next = this.store.data.tasks.find((t) => t.status === 'WAITING')
      if (!next) break
      await this.spawnTask(next)
    }
  }

  private async spawnTask(task: DownloadTask): Promise<void> {
    const settings = this.store.data.settings
    try {
      fs.mkdirSync(task.saveDir, { recursive: true })
    } catch {
      task.status = 'FAILED'
      task.error = '保存目录不可用，请更换保存位置。'
      this.persist()
      this.emit()
      return
    }

    const cookieFile = await writeCookiesToFileForUrl(task.url)
    const args = buildDownloadArgs(
      task.url,
      task.title,
      task.selector,
      task.saveDir,
      settings,
      task.audioFormat,
      task.height,
      cookieFile
    )
    let child: ChildProcess
    try {
      child = spawn(ytdlpPath(), args, { windowsHide: true })
    } catch (e) {
      task.status = 'FAILED'
      task.error = `无法启动下载引擎：${(e as Error).message}`
      this.persist()
      this.emit()
      return
    }

    task.status = 'DOWNLOADING'
    task.error = undefined
    task.errorDetail = undefined
    this.procs.set(task.id, child)
    this.emit()

    let stderrBuf = ''
    let stdoutBuf = ''
    let lastEmit = 0
    let baseBytes = 0
    let prevStreamStatus = ''

    const touch = (force = false) => {
      const now = Date.now()
      if (!force && now - lastEmit < 400) return
      lastEmit = now
      this.persist()
      this.emit()
    }

    const handleLine = (line: string): void => {
      if (line.startsWith('VGFILE|')) {
        task.filePath = line.slice('VGFILE|'.length).trim()
        return
      }
      if (line.startsWith('VGP|')) {
        const [, status, down, total, totalEst, speed, eta, percent] = line.split('|')
        const num = (v?: string): number => {
          const n = Number(v)
          return Number.isFinite(n) && n > 0 ? n : 0
        }
        const streamDown = num(down)
        const streamTotal = num(total) || num(totalEst)

        task.downloaded = baseBytes + streamDown
        task.total = baseBytes + streamTotal
        task.speed = num(speed)
        const etaNum = Number(eta)
        task.eta = Number.isFinite(etaNum) && etaNum > 0 ? etaNum : null

        if (status === 'finished') {
          if (prevStreamStatus !== 'finished') {
            baseBytes += Math.max(streamDown, streamTotal)
          }
        } else {
          const pct = Number(percent)
          const computed =
            task.total > 0
              ? (task.downloaded / task.total) * 100
              : Number.isFinite(pct)
                ? pct
                : 0
          if (computed > task.progress) task.progress = Math.min(99.5, computed)
        }
        prevStreamStatus = status ?? ''
        touch()
        return
      }
      if (/\[Merger\]|Merging formats|\[ExtractAudio\]/i.test(line)) {
        task.status = 'MERGING'
        task.speed = 0
        task.eta = null
        touch(true)
      }
    }

    child.stdout?.on('data', (chunk: Buffer) => {
      stdoutBuf += chunk.toString()
      const lines = stdoutBuf.split(/\r?\n/)
      stdoutBuf = lines.pop() ?? ''
      lines.forEach(handleLine)
    })

    child.stderr?.on('data', (chunk: Buffer) => {
      stderrBuf += chunk.toString()
      if (stderrBuf.length > 20000) stderrBuf = stderrBuf.slice(-20000)
    })

    const finish = (code: number | null): void => {
      this.procs.delete(task.id)
      task.speed = 0
      task.eta = null

      const stillExists = this.find(task.id)
      if (!stillExists) {
        this.pump()
        return
      }

      if (this.shuttingDown) {
        task.status = 'PAUSED'
        task.speed = 0
        task.eta = null
        this.store.flush()
        return
      }

      if (this.cancelRequested.has(task.id)) {
        this.cancelRequested.delete(task.id)
        task.status = 'CANCELLED'
      } else if (this.pauseRequested.has(task.id)) {
        this.pauseRequested.delete(task.id)
        task.status = 'PAUSED'
      } else if (code === 0) {
        task.status = 'COMPLETED'
        task.progress = 100
        task.completedAt = Date.now()
        if (task.filePath && fs.existsSync(task.filePath)) {
          try {
            task.total = fs.statSync(task.filePath).size
            task.downloaded = task.total
          } catch {
            // 忽略
          }
        }
      } else {
        const mapped = friendlyError(stderrBuf)
        task.status = 'FAILED'
        task.error = mapped.message
        task.errorDetail = mapped.detail
      }

      this.persist()
      this.emit()
      this.pump()
    }

    child.on('error', (e) => {
      stderrBuf += `\n${e.message}`
    })
    child.on('close', finish)
  }

  openFolder(id: string): string | null {
    const task = this.find(id)
    if (!task) return null
    if (task.filePath && fs.existsSync(task.filePath)) return path.dirname(task.filePath)
    return task.saveDir
  }

  shutdown(): void {
    this.shuttingDown = true
    for (const [id, proc] of this.procs) {
      const task = this.find(id)
      if (task && (task.status === 'DOWNLOADING' || task.status === 'MERGING')) {
        task.status = 'PAUSED'
        task.speed = 0
        task.eta = null
      }
      proc.kill()
    }
    this.procs.clear()
    this.store.flush()
  }
}
