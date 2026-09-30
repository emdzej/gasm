import { defineConfig } from 'vitepress'

const repo = 'https://github.com/emdzej/gasm'

export default defineConfig({
  title: 'gasm',
  description: 'Game assembly: write a game once, compile it to WebAssembly, run it everywhere.',
  lang: 'en-US',
  cleanUrls: true,
  lastUpdated: true,
  // /play/ is the web runner (static files copied in by scripts/build-site.sh)
  ignoreDeadLinks: [/^\/play/],
  head: [
    ['link', { rel: 'icon', href: '/favicon.svg', type: 'image/svg+xml' }],
    ['meta', { name: 'theme-color', content: '#88c0d0' }],
    ['meta', { property: 'og:title', content: 'gasm: game assembly' }],
    ['meta', { property: 'og:description', content: 'Portable games on WebAssembly: one .wasm, native and browser runners, WebGPU and netplay.' }],
  ],
  themeConfig: {
    logo: '/favicon.svg',
    nav: [
      { text: 'Guide', link: '/guide/' },
      { text: 'How it works', link: '/docs/how-it-works' },
      { text: 'Develop', link: '/dev/' },
      { text: 'ABI', link: '/docs/abi' },
      { text: 'Demos', link: '/demos/' },
    ],
    sidebar: {
      '/guide/': [{ text: 'Guide', items: [{ text: 'User guide', link: '/guide/' }] }],
      '/docs/': [
        { text: 'Documentation', items: [
          { text: 'How it works', link: '/docs/how-it-works' },
          { text: 'ABI v0 specification', link: '/docs/abi' },
        ] },
      ],
      '/dev/': [
        { text: 'Developer guide', items: [
          { text: 'Overview', link: '/dev/' },
          { text: 'Writing games', link: '/dev/games' },
          { text: 'Writing runners', link: '/dev/runners' },
          { text: 'Packages', link: '/dev/packages' },
          { text: 'Badge', link: '/dev/badge' },
          { text: 'Testing & contributing', link: '/dev/testing' },
        ] },
        { text: 'Reference', items: [{ text: 'ABI v0', link: '/docs/abi' }] },
      ],
    },
    outline: { level: [2, 3] },
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: repo }],
    editLink: { pattern: `${repo}/edit/main/site/:path`, text: 'Edit this page on GitHub' },
    footer: {
      message: 'MIT licensed. The NES demo core is tetanes-core (MIT/Apache-2.0).',
      copyright: 'gasm: game assembly',
    },
  },
})
