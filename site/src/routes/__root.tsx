import { HeadContent, Outlet, createRootRoute } from '@tanstack/react-router'

import '../styles.css'

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { title: 'GLIMPS — zero-config smart terminal output formatter' },
      {
        name: 'description',
        content:
          "GLIMPS is a zero-config smart terminal output formatter. It marks where each command's output begins and colors what it can confidently recognize — JSON, logs, HTTP, diffs, stack traces, and more.",
      },
      { name: 'theme-color', content: '#1b1c22' },
    ],
    links: [{ rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }],
  }),
  component: RootComponent,
})

function RootComponent() {
  return (
    <>
      <HeadContent />
      <Outlet />
    </>
  )
}
