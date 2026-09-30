import type { ProcExit } from './gasm-host.js';

/** Asset sources for Worker mode. Memory entries are explicit; the rest are folder entries. */
export type WorkerAssetSpec =
  | { kind: 'memory'; record: Record<string, Uint8Array> }
  | { kind: 'files'; entries: [name: string, file: Blob][]; prefix?: string }   // lazy, FileReaderSync
  | { kind: 'opfs'; dir: string; prefix?: string };                              // lazy, FileSystemSyncAccessHandle

export interface WorkerStats {
  frames: number; presented: number; width: number; height: number;
  videoHash: number; audioHash: number; audioFrames: number;
}

export interface FramesResult {
  /** The latest 2D frame, if a new one was presented. */
  frame: { rgba: Uint8ClampedArray; width: number; height: number } | null;
  stats: WorkerStats;
  frameIndex: number;
  frameRate: number;
}

/** A gasm guest running in a dedicated Worker (guests without gasm:gfx). No COOP/COEP needed. */
export declare class GasmWorker {
  static start(options: {
    wasm: ArrayBuffer | Uint8Array;
    assets?: WorkerAssetSpec[];
    params?: Record<string, string>;
    /** IndexedDB namespace for gasm:storage (null: in-memory). */
    storage?: string | null;
    allowNet?: boolean;
    hashing?: boolean;
    virtualTime?: boolean;
    onLog?: (message: string) => void;
    onAudio?: (samples: Float32Array, rate: number, channels: number) => void;
    url?: URL | string;
  }): Promise<GasmWorker>;
  readonly worker: Worker;
  frameRate: number;
  stats: WorkerStats | null;
  /** Run one frame per entry (each [pad0..pad3]); only the last is shown. Rejects with ProcExit on exit. */
  frames(steps: number[][], show?: boolean): Promise<FramesResult>;
  /** gasm_exit (flush saves), close sockets, terminate. */
  exit(timeoutMs?: number): Promise<void>;
}
export type { ProcExit };
