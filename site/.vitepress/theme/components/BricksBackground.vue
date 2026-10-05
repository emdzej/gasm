<script setup lang="ts">
// The home page's background is a gasm game: guests/bricks (a brick breaker that plays
// itself, in gasm's colours, 26 KB) on @emdzej/gasm-host, the player's copy under /play/. Its frames
// have a transparent background, so the page's colours (light, dark) show through;
// the picture takes the hero's shape (params w, h: 360 pixels high), the ball and
// paddle the theme's colour (fg). It plays by itself. Paused while off screen or in a hidden tab; one still frame with reduced motion.
import { onBeforeUnmount, onMounted } from 'vue'
import { withBase } from 'vitepress'

const HEIGHT = 360
let cleanup = () => {}

onMounted(async () => {
  // made here, not rendered by Vue: the server has nothing to draw, and it stays out of hydration
  const home = document.querySelector('.VPHome')
  if (!home) return
  const el = document.createElement('div')
  el.className = 'gasm-bricks-bg'
  el.setAttribute('aria-hidden', 'true')
  const c = document.createElement('canvas')
  el.append(c)
  home.prepend(el)
  // as tall as the hero, so the paddle plays just under its buttons, above the features
  const hero = home.querySelector('.VPHero') as HTMLElement | null
  const fit = () => { if (hero) el.style.height = `${hero.offsetTop + hero.offsetHeight}px` }
  fit()
  cleanup = () => el.remove()
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches
  const play = new URL(withBase('/play/'), location.href)
  let GasmHost: any, wasm: ArrayBuffer
  try {
    ({ GasmHost } = await import(/* @vite-ignore */ new URL('gasm-host.js', play).href))
    wasm = await (await fetch(new URL('build/bricks.wasm', play))).arrayBuffer()
  } catch (e) {
    return   // no runner here (a dev server without /play/): no background
  }
  const ctx = c.getContext('2d')!
  let host: any = null, size = [0, 0], generation = 0, shownFg = ''

  // (re)start the game for the box's shape: frames HEIGHT high, as wide as the box's aspect
  const start = async () => {
    const r = el.getBoundingClientRect()
    if (r.width < 1 || r.height < 1) return
    const w = Math.max(160, Math.min(1600, Math.round(HEIGHT * r.width / r.height)))
    const fg = document.documentElement.classList.contains('dark') ? 'f2f4f8' : '2d333b'   // ball and paddle
    if (host && fg === shownFg && Math.abs(w - size[0]) / size[0] < 0.15) return   // close enough: keep playing
    shownFg = fg
    const gen = ++generation
    const h = new GasmHost({
      params: { w: String(w), h: String(HEIGHT), fg },
      onLog: () => {},
      onPresent: (rgba: Uint8ClampedArray, fw: number, fh: number) => {
        if (c.width !== fw || c.height !== fh) { c.width = fw; c.height = fh }
        ctx.putImageData(new ImageData(rgba, fw, fh), 0, 0)
      },
    })
    await h.load(wasm)
    if (gen !== generation) return
    host = h
    size = [w, HEIGHT]
    if (reduce) for (let i = 0; i < 90; i++) host.frame()   // a still frame, mid-game
  }
  await start()

  const resized = new ResizeObserver(() => { fit(); start() })
  resized.observe(el)
  if (hero) resized.observe(hero)
  // the light / dark switch: the ball and paddle change colour
  const theme = new MutationObserver(() => { start() })
  theme.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
  if (reduce) {
    cleanup = () => { resized.disconnect(); theme.disconnect(); el.remove() }
    return
  }

  let visible = true, raf = 0, last = performance.now(), acc = 0
  const seen = new IntersectionObserver(([e]) => { visible = e.isIntersecting })
  seen.observe(el)
  const tick = (now: number) => {
    raf = requestAnimationFrame(tick)
    const dt = Math.min(now - last, 100)
    last = now
    if (!host || !visible || document.hidden) return
    acc += dt
    // fixed 60 Hz steps whatever the display's rate
    for (let n = 0; acc >= 1000 / 60 && n < 4; n++) { acc -= 1000 / 60; host.frame() }
  }
  raf = requestAnimationFrame(tick)
  cleanup = () => { cancelAnimationFrame(raf); seen.disconnect(); resized.disconnect(); theme.disconnect(); el.remove() }
})

onBeforeUnmount(() => cleanup())
</script>

<template><span hidden /></template>
