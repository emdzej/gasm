export { SPLASH_FRAMES, SPLASH_H, SPLASH_HOLD, SPLASH_W, splashFrame } from './gasm-host.js';

export interface Splash {
  /** The game is loaded: play the last frames; resolves when the splash is over. */
  ready(): Promise<void>;
  /** Stop at once (resolves `done`). */
  cancel(): void;
  done: Promise<void>;
}

/**
 * The gasm splash screen while a game loads: on a canvas of its own (2D context, sharp
 * pixels scaled by CSS) or through `draw(rgba, width, height)`. It holds on the logo
 * until ready(); a key or click shortens it unless `skipOnInput` is false.
 */
export declare function playSplash(
  target: HTMLCanvasElement | OffscreenCanvas | ((rgba: Uint8ClampedArray, width: number, height: number) => void),
  options?: { skipOnInput?: boolean; inputTarget?: EventTarget },
): Splash;
