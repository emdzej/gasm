// Types for @emdzej/gasm-host. ABI: https://gasm.emdzej.pl/docs/abi

export declare const ABI_VERSION: 0;
export declare const FNV_INIT: number;
/** FNV-1a 32-bit, as used by all gasm runners for determinism hashes. */
export declare function fnv32(hash: number, bytes: Uint8Array): number;

/** Thrown out of `load()`/`frame()` when the guest calls WASI proc_exit. */
export declare class ProcExit extends Error {
  readonly code: number;
}

/**
 * A gasm:gfx backend: executes calls that GfxModel already validated, with handles
 * GfxModel assigned (creation order, never reused). Only width/height/beginFrame are
 * required; NullGfx implements nothing else. `used` is set by begin_frame.
 */
export interface GfxBackend {
  width(): number;
  height(): number;
  beginFrame(r: number, g: number, b: number, a: number, show: boolean): boolean;
  used: boolean;
  /** Throw (trap the guest) if the GPU reported a validation error since the last call. */
  checkErrors?(): void;
  createShader?(handle: number, wgsl: string): void;
  createBuffer?(handle: number, size: number, usage: number): void;
  createPipeline?(handle: number, descriptor: object): void;
  /** `meta.resources`: buffer and texture handles the bind group uses. */
  createBindGroup?(handle: number, descriptor: object, meta: { resources: number[] }): void;
  createBindGroupLayout?(handle: number, descriptor: object): void;
  /** `meta` is the validated descriptor ({ width, height, mips, format }). */
  createTexture?(handle: number, descriptor: object, meta: { width: number; height: number; mips: number; format: string }): void;
  createSampler?(handle: number, descriptor: object): void;
  writeBuffer?(buffer: number, offset: number, bytes: Uint8Array): void;
  writeTexture?(texture: number, mip: number, x: number, y: number, width: number, height: number, rgba: Uint8Array): void;
  setPipeline?(pipeline: number): void;
  setBindGroup?(index: number, bindGroup: number, dynamicOffsets: number[]): void;
  /** Rectangles arrive clamped to the drawable. */
  setViewport?(x: number, y: number, width: number, height: number, minDepth: number, maxDepth: number): void;
  setScissorRect?(x: number, y: number, width: number, height: number): void;
  setVertexBuffer?(slot: number, buffer: number, offset: number): void;
  setIndexBuffer?(buffer: number, format: number, offset: number): void;
  draw?(vertexCount: number, instanceCount: number, firstVertex: number, firstInstance: number): void;
  drawIndexed?(indexCount: number, instanceCount: number, firstIndex: number, baseVertex: number, firstInstance: number): void;
  endFrame?(): void;
  /** The guest destroyed an object; free it once nothing uses it. */
  destroy?(handle: number): void;
  /** Show a video_present frame on a canvas the backend owns (2D frames of gfx guests). */
  presentVideo?(rgba: Uint8Array | Uint8ClampedArray, width: number, height: number): void;
}

/**
 * Validation and handles for gasm:gfx, shared by every backend and identical to the
 * native runner: object records, render pass state, draw ranges (GasmHost uses it).
 */
export declare class GfxModel {
  constructor(backend: GfxBackend);
}
export declare const MAX_TEXTURE_SIZE: number;
export declare const OFFSET_ALIGNMENT: number;
export declare const MAX_BIND_GROUPS: number;
export declare const MAX_VERTEX_BUFFERS: number;
/** GASM_BUF_* usage bits. */
export declare const BUF_COPY_DST: number, BUF_INDEX: number, BUF_VERTEX: number, BUF_UNIFORM: number, BUF_STORAGE: number;
/** Clamp [x, x+w) x [y, y+h) to a drawable: [x, y, w, h]. */
export declare function clampRect(x: number, y: number, w: number, h: number, width: number, height: number): [number, number, number, number];

/** Draws nothing (headless, tests); GfxModel still validates every call. */
export declare class NullGfx implements GfxBackend {
  constructor(width?: number, height?: number);
  width(): number;
  height(): number;
  beginFrame(): false;
  used: boolean;
}

/** One frame of raw input for GasmHost.input (null fields: no such device, the import returns -1). */
export interface RawPointer {
  /** drawable pixels */
  x: number; y: number;
  dx: number; dy: number; wheelX: number; wheelY: number;
  /** GASM_MOUSE_* bits */
  buttons: number; pressed: number; released: number;
  /** GASM_POINTER_INSIDE | IS_HIDDEN | IS_LOCKED */
  flags: number;
  /** drawable size, for the frame position */
  drawable: [width: number, height: number];
  /** 2D frames are shown at whole multiples (letterbox), for the frame position */
  integerScale?: boolean;
}
export interface RawGamepad { connected: boolean; standard: boolean; buttons: number[]; axes: number[]; name: string; }
export interface RawInput {
  /** KEY_STATE_BYTES bitset indexed by GASM_KEY_* */
  keys: Uint8Array | null;
  /** [code, down] since the previous frame, in order */
  keyEvents: [code: number, down: boolean][];
  pointer: RawPointer | null;
  /** 4 slots */
  gamepads: RawGamepad[] | null;
}
/** GASM_KEY_* code -> W3C KeyboardEvent.code name (index 0 unused). */
export declare const KEY_CODES: readonly string[];
/** GASM_KEY_* code for a KeyboardEvent.code (0 if none). */
export declare function keyCode(code: string): number;
/** A KeyboardEvent.code as keymaps name it (OSLeft -> MetaLeft). */
export declare function normalizeCode(code: string): string;
export declare const KEY_STATE_BYTES: number;
export declare const POINTER_BYTES: number;
export declare const GAMEPAD_BYTES: number;
export declare const GAMEPAD_BUTTONS: number;
export declare const GAMEPAD_AXES: number;
export declare const INPUT_KEYS_RAW: number;
export declare const INPUT_POINTER_HIDDEN: number;
export declare const INPUT_POINTER_LOCKED: number;
/** A drawable position mapped into a frame (as letterboxed by the runners). */
export declare function framePosition(x: number, y: number, drawable: [number, number], frame: [number, number], integerScale?: boolean, aspect?: [number, number] | null): [number, number];
/** Where a frame lands in an output, centred: [left, top, scaleX, scaleY]. `aspect`: the frame's display aspect (video_set_aspect), null = square pixels. */
export declare function letterbox(output: [number, number], frame: [number, number], integerScale?: boolean, aspect?: [number, number] | null): [number, number, number, number];
/** navigator.getGamepads() as RawInput.gamepads. */
export declare function browserGamepads(): RawGamepad[];
/** Virtual pads (GASM_BTN_* masks) from raw gamepads: standard buttons, left stick as d-pad. */
export declare function gamepadPads(gamepads: RawGamepad[]): number[];
/**
 * Collects keyboard, pointer and gamepads on a page:
 *   const input = new BrowserInput(canvas).attach();
 *   host.input = input.frame(true);   // before each frame (true: first of a batch)
 *   input.setMode(host.inputMode);    // after: hide / lock the cursor as the guest asked
 */
export declare class BrowserInput {
  constructor(element: HTMLElement, options?: { ignore?: (e: Event) => boolean });
  attach(): this;
  detach(): void;
  /** Follow a new canvas. */
  setElement(element: HTMLElement): void;
  setMode(flags: number): void;
  frame(first?: boolean): RawInput;
  /** The page shows 2D frames at whole multiples (the pointer's frame position follows). */
  integerScale: boolean;
}

/** gasm:storage backend. */
export interface StorageBackend {
  get(key: string): Uint8Array | undefined;
  /** All keys, sorted (gasm:storage count/key). */
  keys(): string[];
  /** Store a value; throws StorageError (its code is returned to the guest). */
  set(key: string, value: Uint8Array): void;
  delete(key: string): boolean;
  /** Wait for writes in flight (IdbStorage). */
  flush?(): Promise<unknown>;
  close?(): void;
}

export declare const STORAGE_MAX_VALUE: number;
export declare const STORAGE_QUOTA: number;
/** GASM_STORAGE_ERR_* codes. */
export declare const STORAGE_ERR_KEY: -1, STORAGE_ERR_SIZE: -2, STORAGE_ERR_QUOTA: -3, STORAGE_ERR_IO: -4;
export declare function validKey(key: string): boolean;
export declare class StorageError extends Error {
  constructor(code: number, message: string);
  /** GASM_STORAGE_ERR_* */
  readonly code: number;
}

/** In-memory store (headless runs: reproducible). Set `persist` to mirror writes elsewhere. */
export declare class MemoryStorage implements StorageBackend {
  constructor(entries?: Iterable<[string, Uint8Array]>);
  get(key: string): Uint8Array | undefined;
  keys(): string[];
  set(key: string, value: Uint8Array): void;
  delete(key: string): boolean;
  close(): void;
  persist?: (op: 'put' | 'delete', key: string, value?: Uint8Array) => void;
}

/** IndexedDB-backed store for browsers, preloaded so reads are synchronous. */
export declare class IdbStorage extends MemoryStorage {
  static open(namespace: string, log?: (message: string) => void): Promise<IdbStorage>;
  flush(): Promise<unknown>;
}

/** gasm:net over the platform WebSocket. */
export declare const NET_CONNECTING: 0, NET_OPEN: 1, NET_CLOSED: 2, NET_ERROR: 3;
export declare const MAX_CONNECTIONS: number;
export declare class NetConnections {
  constructor(allowed: boolean, log: (message: string) => void);
  /** Close every connection with a handshake (bounded wait), delivering queued messages. */
  closeAll(timeoutMs?: number): Promise<unknown>;
}

/** Headless virtual time (frame-derived, monotonic across frame rate changes). */
export declare class VirtualClock {
  /** Time in ms at the start of `frame` at frame rate `rate`. */
  at(frame: number, rate: number): number;
}
/** The fixed random_get sequence of reproducible runs (splitmix64). */
export declare class Splitmix {
  constructor(seed?: bigint);
  fill(bytes: Uint8Array): void;
}

/** Streaming linear resampler (interleaved input -> stereo at dstRate). */
export declare class Resampler {
  constructor(dstRate: number);
  process(samples: Float32Array, srcRate: number, channels: number): Float32Array;
}

export interface GasmHostOptions {
  /** Read-only assets: a GasmAssetProvider (see AssetTable), or `{ name: bytes }`. */
  assets?: GasmAssetProvider | Record<string, Uint8Array>;
  /** Launch parameters (gasm.param). */
  params?: Record<string, string>;
  /** gasm:gfx backend. Default: NullGfx. */
  gfx?: GfxBackend;
  /** gasm:storage backend. Default: MemoryStorage. */
  storage?: StorageBackend;
  /** Allow gasm:net connections (WebSocket). Default false. */
  allowNet?: boolean;
  /** 2D frames from gasm.video_present (RGBA8, tightly packed). */
  onPresent?: (rgba: Uint8ClampedArray, width: number, height: number, aspect: [num: number, den: number] | null) => void;
  /** Audio from gasm.audio_push: interleaved f32 at the guest's rate. */
  onAudio?: (samples: Float32Array, rate: number, channels: number) => void;
  onLog?: (message: string) => void;
  /** gasm.set_title, after a frame that changed it (cleaned; null = back to the default). */
  onTitle?: (title: string | null) => void;
  /** Buttons held on virtual pad `player` (0..3) as a bitmask. */
  getPad?: (player: number) => number;
  /** Reproducible (headless) mode: frame-derived time_ms and WASI clocks, a fixed
   *  random_get sequence. Also enables hashing. */
  virtualTime?: boolean;
  /** Run guests that export gasm_run that way (JSPI). Default: when the engine has JSPI. */
  stackSwitching?: boolean;
}

/** One frame of a batch for GasmHost.runFrames / GasmWorker.frames. */
export interface FrameStep {
  pads?: number[];
  /** text_input for the frame (undefined: keep host.text) */
  text?: string | null;
  input?: RawInput;
}

/**
 * Hosts one gasm guest. Load it, then call `frame()` at `frameRate` Hz.
 *
 *   const host = new GasmHost({ onPresent, getPad });
 *   await host.load(await (await fetch('game.wasm')).arrayBuffer());
 *   setInterval(() => host.frame(), 1000 / host.frameRate);
 */
/** Whether this JS engine has JSPI, so gasm_run guests can run (stack switching). */
export declare const STACK_SWITCHING: boolean;
/** Longest title gasm.set_title keeps, in UTF-8 bytes (256). */
export declare const TITLE_MAX_BYTES: number;
/** set_title text as runners show it: control and bidi characters removed, cut to 256 bytes; null if empty. */
export declare function cleanTitle(text: string): string | null;
/** A module's built-in title (custom section gasm.title), cleaned; null if none. */
export declare function staticTitle(module: WebAssembly.Module): string | null;

export declare class GasmHost {
  constructor(options?: GasmHostOptions);
  /** The guest's set_title (cleaned); null: the runner's default. */
  readonly title: string | null;
  /** The module's gasm.title section (cleaned), after load(); null if none. */
  readonly staticTitle: string | null;
  /** This guest runs through gasm_run (stack switching, JSPI): use frameAsync / runFramesAsync. */
  readonly switching: boolean;
  /** One frame of any guest (for gasm_run guests: resume until the next yield_frame). */
  frameAsync(): Promise<void>;
  /** runFrames for any guest. */
  runFramesAsync(steps: FrameStep[], show?: boolean): Promise<{ video: boolean }>;
  /** The guest's video_set_aspect [num, den]; null: square pixels. */
  readonly aspect: [number, number] | null;
  assets: GasmAssetProvider;
  params: Record<string, string>;
  gfx: GfxBackend;
  storage: StorageBackend;
  getPad: (player: number) => number;
  /** Text typed since the previous frame, for text_input; set it before each frame. null = no keyboard (-1). */
  text: string | null;
  /** Raw keyboard, pointer and gamepads for the next frame (BrowserInput.frame()). */
  input: RawInput;
  /** GASM_INPUT_* flags the guest asked for (input_mode). Runners: skip keymap pads with KEYS_RAW, apply the cursor mode. */
  inputMode: number;
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
  /** The guest's exports (after load). */
  exports: WebAssembly.Exports | null;
  /** gasm:net connections (closeAll() on shutdown). */
  net: NetConnections;
  /** Why the guest can't be called any more (it trapped or exited), else null. */
  dead: Error | null;
  /** Compile (bytes) or take a compiled module, instantiate, run init. */
  load(wasm: BufferSource | WebAssembly.Module): Promise<void>;
  /** One frame. Throws the guest's trap / ProcExit; after that, throws without calling it. */
  frame(): void;
  /** A catch-up batch: one frame per step, only the last shown; blits 2D frames into a
   *  gfx canvas (presentVideo). Returns whether a new 2D frame was presented. */
  runFrames(steps: FrameStep[], show?: boolean): { video: boolean };
  /** The player is quitting: calls the guest's optional gasm_exit (flush saves). The guest is not called again. */
  exit(): void;
  /** exit(), then close network connections (flushing them) and the storage. */
  shutdown(): Promise<void>;
}

/** Keyboard layouts: "<pad 1-4> <button> <key code>..." per line (KeyboardEvent.code names). */
export declare const BUTTONS: readonly string[];
export declare const DEFAULT_KEYMAP: string;
export interface KeyBinding { pad: number; bit: number }
export declare function parseKeymap(text: string): { bindings: Map<string, KeyBinding[]>; errors: string[] };
/** Pads from held keys; keyboard pad N (N >= 2) only while fewer than N gamepads are connected. */
export declare function keyboardPads(bindings: Map<string, KeyBinding[]>, held: Iterable<string>, gamepads?: number): number[];

/** Synchronous asset source (the ABI is synchronous). */
export interface GasmAssetProvider {
  /** Size in bytes (any size), or -1 if missing. */
  size(name: string): number;
  /** Copy bytes from offset into dst; bytes copied, or -1 if missing. */
  readAt(name: string, offset: number, dst: Uint8Array): number;
  /** All names, sorted by UTF-8 bytes (asset_count / asset_name). */
  names?(): string[];
}
/** UTF-8 byte order (= code point order) for sort(). */
export declare function byCodePoint(a: string, b: string): number;
export interface AssetSource { size(): number; readAt(offset: number, dst: Uint8Array): number }
/** Asset table with gasm's naming rules: exact names win; folder entries also match case-insensitively (ASCII). */
export declare class AssetTable implements GasmAssetProvider {
  constructor(log?: (message: string) => void);
  add(name: string, source: AssetSource, options?: { fromDir?: boolean }): boolean;
  merge(table: AssetTable, options?: { fromDir?: boolean }): this;
  finish(): this;
  size(name: string): number;
  readAt(name: string, offset: number, dst: Uint8Array): number;
  names(): string[];
}
export declare function isAssetProvider(value: unknown): value is GasmAssetProvider;
export declare function bytesSource(bytes: Uint8Array): AssetSource;
export declare function memoryAssets(record?: Record<string, Uint8Array>): AssetTable;
export interface LoadProgress { done: number; total: number; bytes: number; name: string }
export interface FolderOptions { prefix?: string; onProgress?: (p: LoadProgress) => void; log?: (message: string) => void }
/** Folder from showDirectoryPicker(), preloaded into memory (main-thread mode). */
export declare function directoryHandleAssets(handle: FileSystemDirectoryHandle, options?: FolderOptions): Promise<AssetTable>;
/** [relative name, File] entries of a directory handle (sorted, hidden entries skipped). */
export declare function directoryHandleEntries(handle: FileSystemDirectoryHandle): Promise<[string, File][]>;
/** Folder from <input webkitdirectory> (root segment stripped), preloaded into memory. */
export declare function fileListAssets(files: FileList | File[], options?: FolderOptions): Promise<AssetTable>;
export declare function fileListEntries(files: FileList | File[]): [string, File][];
export declare function preloadAssets(entries: [string, Blob][], options?: FolderOptions): Promise<AssetTable>;
/** Worker only: lazy reads from File/Blob objects via FileReaderSync. */
export declare function fileAssets(entries: [string, Blob][], options?: { prefix?: string; log?: (message: string) => void }): AssetTable;
/** Worker only: lazy synchronous reads from an OPFS directory (FileSystemSyncAccessHandle). */
export declare function opfsAssets(dir: string | FileSystemDirectoryHandle, options?: { prefix?: string; log?: (message: string) => void }): Promise<AssetTable>;
