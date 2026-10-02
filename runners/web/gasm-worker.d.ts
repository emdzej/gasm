import type { FrameStep, ProcExit } from './gasm-host.js';

/** Asset sources for Worker mode. Memory entries are explicit; the rest are folder entries. */
export type WorkerAssetSpec =
  | { kind: 'memory'; record: Record<string, Uint8Array> }
  | { kind: 'files'; entries: [name: string, file: Blob][]; prefix?: string }   // lazy, FileReaderSync
  | { kind: 'opfs'; dir: string | FileSystemDirectoryHandle; prefix?: string };  // lazy, FileSystemSyncAccessHandle

export interface WorkerStats {
  frames: number; presented: number; width: number; height: number;
  videoHash: number; audioHash: number; audioFrames: number;
}

export interface FramesResult {
  /** The latest 2D frame, if a new one was presented. */
  frame: { rgba: Uint8ClampedArray; width: number; height: number; aspect: [number, number] | null } | null;
  stats: WorkerStats;
  frameIndex: number;
  frameRate: number;
}

/**
 * A gasm guest running in a dedicated Worker. No COOP/COEP needed. gasm:gfx guests need
 * `canvas` (canvas.transferControlToOffscreen()) and WebGPU in the worker; start() rejects
 * with an error mentioning "WebGPU" otherwise (run on the main thread then).
 */
export declare class GasmWorker {
  static start(options: {
    /** A compiled module (shared with the worker, no copy) or bytes (transferred: the
     *  ArrayBuffer is detached afterwards; pass a copy to keep it). */
    wasm: WebAssembly.Module | ArrayBuffer | Uint8Array;
    assets?: WorkerAssetSpec[];
    params?: Record<string, string>;
    /** IndexedDB namespace for gasm:storage (null: in-memory). */
    storage?: string | null;
    allowNet?: boolean;
    /** The page forwards typed text (FrameStep.text); false: text_input returns -1. */
    keyboard?: boolean;
    /** gasm:gfx: an OffscreenCanvas from transferControlToOffscreen(), and its display size in device pixels. */
    canvas?: OffscreenCanvas | null;
    size?: [width: number, height: number] | null;
    hashing?: boolean;
    virtualTime?: boolean;
    onLog?: (message: string) => void;
    onAudio?: (samples: Float32Array, rate: number, channels: number) => void;
    /** gasm.set_title changed (cleaned; null = the default). */
    onTitle?: (title: string | null) => void;
    url?: URL | string;
  }): Promise<GasmWorker>;
  readonly worker: Worker;
  frameRate: number;
  /** The guest's set_title as of the last batch (null: default). */
  readonly title: string | null;
  /** GASM_INPUT_* flags the guest asked for, after the last batch. */
  inputMode: number;
  stats: WorkerStats | null;
  /** Run one frame per step (or per pads array); only the last is shown. Rejects with
   *  ProcExit on exit; after an exit or a trap every call rejects. */
  frames(steps: (FrameStep | number[])[], show?: boolean, options?: {
    /** gasm:gfx canvas display size in device pixels. */
    size?: [width: number, height: number] | null;
  }): Promise<FramesResult>;
  /** gasm_exit (flush saves), close sockets and storage, terminate. Safe to call twice. */
  exit(timeoutMs?: number): Promise<void>;
}
export type { ProcExit };
