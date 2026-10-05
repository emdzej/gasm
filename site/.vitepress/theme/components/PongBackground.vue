<script setup lang="ts">
// The home page's background is a gasm game: guests/pong (attract-mode Pong in the
// icon's colours, 23 KB) on @emdzej/gasm-host, the player's copy under /play/. Its
// frames have a transparent background, so the page's colours (light, dark) show
// through; the court takes the hero's shape (params w, h: 120 pixels high). The
// right paddle follows the pointer while it moves over the hero. Paused while off
// screen or in a hidden tab; one still frame with reduced motion.
import { onBeforeUnmount, onMounted } from 'vue'
import { withBase } from 'vitepress'

const HEIGHT = 120
let cleanup = () => {}

onMounted(async () => {
  // made here, not rendered by Vue: the server has nothing to draw, and it stays out of hydration
  const home = document.querySelector('.VPHome')
  if (!home) return
  const el = document.createElement('div')
  el.className = 'gasm-pong-bg'
  el.setAttribute('aria-hidden', 'true')
  const c = document.createElement('canvas')
  el.append(c)
  home.prepend(el)
  cleanup = () => el.remove()
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches
  const play = new URL(withBase('/play/'), location.href)
  let GasmHost: any, wasm: ArrayBuffer
  try {
    ({ GasmHost } = await import(/* @vite-ignore */ new URL('gasm-host.js', play).href))
    wasm = await (await fetch(new URL('build/pong.wasm', play))).arrayBuffer()
  } catch (e) {
    return   // no runner here (a dev server without /play/): no background
  }
  const ctx = c.getContext('2d')!
  const pointer = { x: 0, y: 0, dx: 0, dy: 0, wheelX: 0, wheelY: 0, buttons: 0, pressed: 0, released: 0, flags: 0, drawable: [1, 1], integerScale: false }
  let host: any = null, size = [0, 0], generation = 0

  // (re)start the game for the box's shape: frames HEIGHT high, as wide as the box's aspect
  const start = async () => {
    const r = el.getBoundingClientRect()
    if (r.width < 1 || r.height < 1) return
    const w = Math.max(64, Math.min(960, Math.round(HEIGHT * r.width / r.height)))
    if (host && Math.abs(w - size[0]) / size[0] < 0.15) return   // close enough: keep playing
    const gen = ++generation
    const h = new GasmHost({
      params: { w: String(w), h: String(HEIGHT) },
      onLog: () => {},
      onPresent: (rgba: Uint8ClampedArray, fw: number, fh: number) => {
        if (c.width !== fw || c.height !== fh) { c.width = fw; c.height = fh }
        ctx.putImageData(new ImageData(rgba, fw, fh), 0, 0)
      },
    })
    h.input = { pointer }
    await h.load(wasm)
    if (gen !== generation) return
    host = h
    size = [w, HEIGHT]
    pointer.drawable = size
    if (reduce) for (let i = 0; i < 90; i++) host.frame()   // a still frame, mid-rally
  }
  await start()

  // the pointer in the frame's pixels (the canvas fills the box exactly)
  const move = (e: PointerEvent) => {
    const r = c.getBoundingClientRect()
    const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom
    pointer.x = (e.clientX - r.left) / r.width * size[0]
    pointer.y = (e.clientY - r.top) / r.height * size[1]
    pointer.flags = inside ? 1 : 0
  }
  addEventListener('pointermove', move, { passive: true })
  const resized = new ResizeObserver(() => { start() })
  resized.observe(el)
  if (reduce) {
    cleanup = () => { resized.disconnect(); removeEventListener('pointermove', move); el.remove() }
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
  cleanup = () => { cancelAnimationFrame(raf); seen.disconnect(); resized.disconnect(); removeEventListener('pointermove', move); el.remove() }
})

onBeforeUnmount(() => cleanup())
</script>

<template><span hidden /></template>
