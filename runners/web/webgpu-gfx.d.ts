import type { GfxBackend } from './gasm-host.js';

export declare const SAMPLE_COUNT: number;

/**
 * gasm:gfx on the browser's WebGPU. The canvas' CSS size sets the drawable size; for an
 * OffscreenCanvas (Worker mode) call setSize() with the display size.
 */
export declare class WebGpuGfx implements GfxBackend {
  static create(canvas: HTMLCanvasElement | OffscreenCanvas, log?: (message: string) => void): Promise<WebGpuGfx>;
  /** OffscreenCanvas: display size in device pixels. */
  setSize(width: number, height: number): void;
  /** Letterboxed blit of a video_present frame onto this canvas (frames that didn't use gfx). */
  presentVideo(rgba: Uint8Array | Uint8ClampedArray, width: number, height: number): void;
  /** Set when the guest calls begin_frame; the runner resets it before each frame. */
  used: boolean;
  /** Swapchain format (navigator.gpu.getPreferredCanvasFormat()). */
  readonly format: string;
  width(): number;
  height(): number;
  createShader(wgsl: string): number;
  createBuffer(size: number, usage: number): number;
  createPipeline(descriptor: object): number;
  createBindGroup(descriptor: object): number;
  createBindGroupLayout(descriptor: object): number;
  /** `meta` is the validated descriptor ({ width, height, mips, format }). */
  createTexture(descriptor: object, meta: { width: number; height: number; mips: number; format: string }): number;
  createSampler(descriptor: object): number;
  writeTexture(texture: number, mip: number, x: number, y: number, width: number, height: number, rgba: Uint8Array): void;
  setBindGroupOffsets(index: number, bindGroup: number, offsets: Uint32Array): void;
  /** Rectangles arrive clamped to the drawable. */
  setViewport(x: number, y: number, width: number, height: number, minDepth: number, maxDepth: number): void;
  setScissorRect(x: number, y: number, width: number, height: number): void;
  writeBuffer(buffer: number, offset: number, bytes: Uint8Array): void;
  beginFrame(r: number, g: number, b: number, a: number, show: boolean): boolean;
  setPipeline(pipeline: number): void;
  setBindGroup(index: number, bindGroup: number): void;
  setVertexBuffer(slot: number, buffer: number, offset: number): void;
  setIndexBuffer(buffer: number, format: number, offset: number): void;
  draw(vertexCount: number, instanceCount: number, firstVertex: number, firstInstance: number): void;
  drawIndexed(indexCount: number, instanceCount: number, firstIndex: number, baseVertex: number, firstInstance: number): void;
  endFrame(): void;
}
