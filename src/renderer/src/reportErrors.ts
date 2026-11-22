const recent: string[] = []
const MAX_ERRORS = 5

function push(text: string): void {
  const line = text.trim()
  if (!line) return
  recent.push(line.slice(0, 2000))
  if (recent.length > MAX_ERRORS) recent.splice(0, recent.length - MAX_ERRORS)
}

/** 在 main.tsx 里调用一次，把前端异常留作上报素材 */
export function installErrorCollector(): void {
  window.addEventListener('error', (event) => {
    const where = event.filename ? ` @ ${event.filename}:${event.lineno}` : ''
    push(`${event.message}${where}`)
  })
  window.addEventListener('unhandledrejection', (event) => {
    const reason = event.reason
    const text = reason instanceof Error ? (reason.stack ?? reason.message) : String(reason)
    push(`unhandledrejection: ${text}`)
  })
}

export function recentErrors(): string[] {
  return [...recent]
}
