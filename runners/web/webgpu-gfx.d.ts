import type { GfxBackend } from './gasm-host.js';

export declare const SAMPLE_COUNT: number;

/**
 * gasm:gfx on the browser's WebGPU (the backend of GasmHost; GfxModel validates). The canvas' CSS size sets the drawable size; for an
 * OffscreenCanvas (Worker mode) call setSize() with the display size.
 */
export declare class WebGpuGfx implements GfxBackend {
  static create(canvas: HTMLCanvasElement | OffscreenCanvas, log?: (message: string) => void): Promise<WebGpuGfx>;
  /** OffscreenCanvas: display size in device pixels. */
  setSize(width: number, height: number): void;
  /** Letterboxed blit of a video_present frame onto this canvas (frames that didn't use gfx). */
  presentVideo(rgba: Uint8Array | Uint8ClampedArray, width: number, height: number, aspect?: [number, number] | null): void;
  /** Set when the guest calls begin_frame; the runner resets it before each frame. */
  used: boolean;
  /** Swapchain format (navigator.gpu.getPreferredCanvasFormat()). */
  readonly format: string;
  width(): number;
  height(): number;
  beginFrame(r: number, g: number, b: number, a: number, show: boolean): boolean;
  /** Throw (trap the guest) if the GPU reported a validation error since the last call. */
  checkErrors(): void;
  /** Free GPU objects of a destroyed handle once no bind group or recorded frame uses them. */
  destroy(handle: number): void;
}
