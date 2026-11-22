import type { ReactElement } from 'react'

const paths: Record<string, ReactElement> = {
  download: <path d="M12 3v12m0 0 4.5-4.5M12 15 7.5 10.5M4 18.5h16" />,
  link: <path d="M10 13.5a4 4 0 0 0 5.7 0l2.8-2.8a4 4 0 0 0-5.7-5.7L11 6.8M14 10.5a4 4 0 0 0-5.7 0L5.5 13.3a4 4 0 0 0 5.7 5.7l1.8-1.8" />,
  folder: <path d="M3.5 7.5a2 2 0 0 1 2-2h3l1.8 2h8.2a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-9Z" />,
  pause: <path d="M9 5v14M15 5v14" />,
  play: <path d="M8 5l11 7-11 7V5Z" />,
  close: <path d="M6 6l12 12M18 6 6 18" />,
  trash: <path d="M4 7h16M9 7V4.5h6V7m-8 0 .8 12.5h8.4L17 7" />,
  refresh: <path d="M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6" />,
  settings: <path d="M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4ZM19.4 13.5l1.6 1.2-1.7 3-1.9-.7a7.6 7.6 0 0 1-1.9 1.1l-.3 2h-3.4l-.3-2a7.6 7.6 0 0 1-1.9-1.1l-1.9.7-1.7-3 1.6-1.2a7.7 7.7 0 0 1 0-2.6L4.6 9.7l1.7-3 1.9.7a7.6 7.6 0 0 1 1.9-1.1l.3-2h3.4l.3 2a7.6 7.6 0 0 1 1.9 1.1l1.9-.7 1.7 3-1.6 1.2a7.7 7.7 0 0 1 0 2.6Z" />,
  check: <path d="M5 12.5 10 17 19 7" />,
  checkCircle: <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm-3.8-9.2 2.6 2.6 5-5.2" />,
  alert: <path d="M12 8v5m0 3.2v.3M10.3 3.9 2.8 17a1.8 1.8 0 0 0 1.5 2.7h15.4a1.8 1.8 0 0 0 1.5-2.7L13.7 3.9a1.9 1.9 0 0 0-3.4 0Z" />,
  video: <path d="M3.5 6.5h11a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-11a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2Zm13 4 5-3v9l-5-3v-3Z" />,
  globe: <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm0 0c2.6-2.2 4-5.4 4-9s-1.4-6.8-4-9c-2.6 2.2-4 5.4-4 9s1.4 6.8 4 9ZM3.5 9h17M3.5 15h17" />,
  clipboard: <path d="M9 5.5h6M9 3.5h6v4H9v-4ZM7 5.5H5.5v15h13v-15H17" />,
  grid: <path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z" />,
  list: <path d="M8 6h12M8 12h12M8 18h12M4 6h.5M4 12h.5M4 18h.5" />,
  chevronUp: <path d="m6 14 6-6 6 6" />,
  chevronDown: <path d="m6 10 6 6 6-6" />,
  chevronRight: <path d="m10 6 6 6-6 6" />,
  music: <path d="M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0Zm11-2a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />,
  rss: <path d="M4 11a9 9 0 0 1 9 9M4 4a16 16 0 0 1 16 16M6 19h.5" />,
  copy: <path d="M9 9h11v11H9zM5 15H4V4h11v1" />,
  clock: <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm0-13v5l3.5 2" />,
  info: <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm0-13v.3M12 12v5" />,
  sort: <path d="M7 4v16m0 0-3.5-3.5M7 20l3.5-3.5M17 20V4m0 0-3.5 3.5M17 4l3.5 3.5" />,
  filter: <path d="M4 5h16l-6 7v6l-4 2v-8L4 5Z" />,
  plus: <path d="M12 5v14M5 12h14" />,
  library: <path d="M4 4v16M9 4v16M13 5l6-1 2 15-6 1-2-15Z" />,
  spinner: <path d="M12 3a9 9 0 1 0 9 9" />
}

export function Icon({ name, size = 18, className }: { name: keyof typeof paths | string; size?: number; className?: string }): ReactElement {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={className}
    >
      {paths[name] ?? null}
    </svg>
  )
}

export function Logo({ size = 34 }: { size?: number }): ReactElement {
  return (
    <span className="logo-mark" style={{ width: size, height: size, borderRadius: size * 0.28 }}>
      <svg width={size * 0.58} height={size * 0.58} viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <path
          d="M12 3v11m0 0 4-4m-4 4-4-4M5 16.5v2A2.5 2.5 0 0 0 7.5 21h9a2.5 2.5 0 0 0 2.5-2.5v-2"
          stroke="#fff"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </span>
  )
}
