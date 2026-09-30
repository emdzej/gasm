// Types for @emdzej/gasm-host. ABI: https://gasm.emdzej.pl/docs/abi

export declare const ABI_VERSION: 0;
export declare const FNV_INIT: number;
/** FNV-1a 32-bit, as used by all gasm runners for determinism hashes. */
export declare function fnv32(hash: number, bytes: Uint8Array): number;

/** Thrown out of `load()`/`frame()` when the guest calls WASI proc_exit. */
export declare class ProcExit extends Error {
  readonly code: number;
}

/** A gasm:gfx backend. `NullGfx` (headless) or `WebGpuGfx` (browser, "@emdzej/gasm-host/webgpu"). */
export interface GfxBackend {
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

/** Draws nothing; allocates handles so guests behave identically (headless, tests). */
export declare class NullGfx implements GfxBackend {
  constructor(width?: number, height?: number);
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

/** gasm:storage backend. */
export interface StorageBackend {
  get(key: string): Uint8Array | undefined;
  /** Returns an error message, or null on success. */
  set(key: string, value: Uint8Array): string | null;
  delete(key: string): boolean;
}

export declare const STORAGE_MAX_VALUE: number;
export declare const STORAGE_QUOTA: number;
export declare function validKey(key: string): boolean;

/** In-memory store (headless runs: reproducible). */
export declare class MemoryStorage implements StorageBackend {
  constructor(entries?: Iterable<[string, Uint8Array]>);
  get(key: string): Uint8Array | undefined;
  set(key: string, value: Uint8Array): string | null;
  delete(key: string): boolean;
}

/** IndexedDB-backed store for browsers, preloaded so reads are synchronous. */
export declare class IdbStorage extends MemoryStorage {
  static open(namespace: string): Promise<IdbStorage>;
}

/** Streaming linear resampler (interleaved input -> stereo at dstRate). */
export declare class Resampler {
  constructor(dstRate: number);
  process(samples: Float32Array, srcRate: number, channels: number): Float32Array;
}

export interface GasmHostOptions {
  /** Read-only assets, e.g. `{ rom: bytes }`. */
  assets?: Record<string, Uint8Array>;
  /** Launch parameters (gasm.param). */
  params?: Record<string, string>;
  /** gasm:gfx backend. Default: NullGfx. */
  gfx?: GfxBackend;
  /** gasm:storage backend. Default: MemoryStorage. */
  storage?: StorageBackend;
  /** Allow gasm:net connections (WebSocket). Default false. */
  allowNet?: boolean;
  /** 2D frames from gasm.video_present (RGBA8, tightly packed). */
  onPresent?: (rgba: Uint8ClampedArray, width: number, height: number) => void;
  /** Audio from gasm.audio_push: interleaved f32 at the guest's rate. */
  onAudio?: (samples: Float32Array, rate: number, channels: number) => void;
  onLog?: (message: string) => void;
  /** Buttons held on virtual pad `player` (0..3) as a bitmask. */
  getPad?: (player: number) => number;
  /** Frame-derived time_ms (deterministic runs). Also enables hashing. */
  virtualTime?: boolean;
}

/**
 * Hosts one gasm guest. Load it, then call `frame()` at `frameRate` Hz.
 *
 *   const host = new GasmHost({ onPresent, getPad });
 *   await host.load(await (await fetch('game.wasm')).arrayBuffer());
 *   setInterval(() => host.frame(), 1000 / host.frameRate);
 */
export declare class GasmHost {
  constructor(options?: GasmHostOptions);
  assets: Record<string, Uint8Array>;
  params: Record<string, string>;
  gfx: GfxBackend;
  storage: StorageBackend;
  getPad: (player: number) => number;
  /** false during catch-up frames: gfx begin_frame then returns 0. */
  showFrame: boolean;
  /** Set by the guest (gasm.set_frame_rate). */
  frameRate: number;
  frameIndex: number;
  width: number;
  height: number;
  /** Last presented 2D frame. */
  rgba: Uint8ClampedArray;
  framesPresented: number;
  hashing: boolean;
  videoHash: number;
  audioHash: number;
  audioFrames: number;
  memory: WebAssembly.Memory | null;
  load(wasm: BufferSource): Promise<void>;
  frame(): void;
  /** The player is quitting: calls the guest's optional gasm_exit (flush saves). */
  exit(): void;
}
