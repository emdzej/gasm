// gasm-splash.js — the gasm splash screen for pages that run games with GasmHost: the
// same frames as gasm-run and the player (lib/splash.js, a moment of Pong turning into
// the gasm logo, about 1.6 s at 60 frames a second). It holds on the logo until the
// page says the game is ready, and a key or click shortens it.
//
//   import { playSplash } from '@emdzej/gasm-host/splash';
//   const splash = playSplash(splashCanvas);      // or playSplash((rgba, w, h) => myDraw(...))
//   const bytes = await (await fetch('game.wasm')).arrayBuffer();
//   await host.load(bytes);                        // compile and load meanwhile
//   await splash.ready();                          // plays the last frames, then resolves
//
// A canvas target is drawn through its 2D context (sharp pixels, scaled by CSS). A
// canvas keeps the kind of context it first got, so give the splash a canvas of its
// own (over the game's, removed afterwards), or pass a draw function that shows the
// RGBA frames your way.

import { SPLASH_FRAMES, SPLASH_H, SPLASH_HOLD, SPLASH_W, splashFrame } from './lib/splash.js';

export { SPLASH_FRAMES, SPLASH_H, SPLASH_HOLD, SPLASH_W, splashFrame };

/**
 * Play the splash on `target` (a canvas, or draw(rgba, width, height)). Options:
 * `skipOnInput` (default true: a key or click shortens it, listening on `inputTarget`,
 * default window). Returns { ready(): Promise (call when the game is loaded; resolves
 * when the splash is over), cancel(), done: Promise }.
 */
export function playSplash(target, { skipOnInput = true, inputTarget = globalThis } = {}) {
  let draw = target;
  if (typeof target !== 'function') {
    const canvas = target;
    canvas.width = SPLASH_W;
    canvas.height = SPLASH_H;
    canvas.style.imageRendering ||= 'pixelated';
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('playSplash: the canvas has no 2D context (it already has another kind: use a canvas of its own)');
    draw = (rgba) => ctx.putImageData(new ImageData(rgba, SPLASH_W, SPLASH_H), 0, 0);
  }
  let frame = 0, ready = false, skip = false, over = false, end;
  const done = new Promise((r) => { end = r; });
  const onInput = () => { skip = true; };
  if (skipOnInput) {
    inputTarget.addEventListener?.('keydown', onInput, true);
    inputTarget.addEventListener?.('pointerdown', onInput, true);
  }
  const finish = () => {
    if (over) return;
    over = true;
    inputTarget.removeEventListener?.('keydown', onInput, true);
    inputTarget.removeEventListener?.('pointerdown', onInput, true);
    end();
  };
  let t0 = performance.now(), shown = -1;
  const step = (now) => {
    if (over) return;
    if (skip) frame = ready ? SPLASH_FRAMES : Math.max(frame, SPLASH_HOLD);
    if (frame >= SPLASH_FRAMES && ready) return finish();
    if (frame !== shown) { draw(splashFrame(frame), SPLASH_W, SPLASH_H); shown = frame; }
    // 60 frames a second whatever the display's rate; hold on the logo until ready
    const due = Math.floor((now - t0) * 60 / 1000);
    if (frame < due && (frame !== SPLASH_HOLD || ready)) frame++;
    if (frame === SPLASH_HOLD && !ready) t0 = now - SPLASH_HOLD * 1000 / 60;
    requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
  return { done, ready: () => { ready = true; return done; }, cancel: finish };
}
