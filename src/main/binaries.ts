import { app } from 'electron'
import path from 'node:path'
import fs from 'node:fs'

const platformDir = (): string => `${process.platform}-${process.arch}`

function candidates(): string[] {
  return [
    path.join(process.resourcesPath, 'bin', platformDir()),
    path.join(app.getAppPath(), 'resources', 'bin', platformDir()),
    path.join(__dirname, '..', '..', 'resources', 'bin', platformDir()),
    path.join(process.cwd(), 'resources', 'bin', platformDir())
  ]
}

function binDir(): string {
  const engineName = process.platform === 'win32' ? 'yt-dlp.exe' : 'yt-dlp'
  const found = candidates().find((dir) => fs.existsSync(path.join(dir, engineName)))
  if (found) return found
  throw new Error('缺少下载引擎组件，请重新安装软件。')
}

export function ytdlpPath(): string {
  const name = process.platform === 'win32' ? 'yt-dlp.exe' : 'yt-dlp'
  const p = path.join(binDir(), name)
  if (!fs.existsSync(p)) {
    throw new Error('缺少下载引擎组件，请重新安装软件。')
  }
  return p
}

export function ffmpegDir(): string {
  return binDir()
}
