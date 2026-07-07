import { HeadContent, Outlet, createRootRoute } from '@tanstack/react-router'

import '../styles.css'
import { asset } from '../lib/seo'

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
      { name: 'robots', content: 'index, follow' },
      { name: 'author', content: 'Krishnarajan' },
      // Open Graph / Twitter defaults (per-route title/description/url override).
      { property: 'og:site_name', content: 'GLIMPS' },
      { property: 'og:type', content: 'website' },
      { property: 'og:image', content: asset('/og.png') },
      { property: 'og:image:width', content: '1200' },
      { property: 'og:image:height', content: '630' },
      {
        property: 'og:image:alt',
        content: 'GLIMPS — zero-config terminal output formatter',
      },
      { name: 'twitter:card', content: 'summary_large_image' },
      { name: 'twitter:image', content: asset('/og.png') },
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
