import type { GfxBackend } from './gasm-host.js';

export declare const SAMPLE_COUNT: number;

/** gasm:gfx on the browser's WebGPU. The canvas' CSS size sets the drawable size. */
export declare class WebGpuGfx implements GfxBackend {
  static create(canvas: HTMLCanvasElement, log?: (message: string) => void): Promise<WebGpuGfx>;
  /** Swapchain format (navigator.gpu.getPreferredCanvasFormat()). */
  readonly format: string;
  width(): number;
  height(): number;
  createShader(wgsl: string): number;
  createBuffer(size: number, usage: number): number;
  createPipeline(descriptor: object): number;
  createBindGroup(descriptor: object): number;
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
