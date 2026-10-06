// gasm-host.js — gasm ABI v0 host for JavaScript (browser and Node).
//
//   const host = new GasmHost({ assets, params, gfx, storage, allowNet, onPresent, onAudio, onLog, getPad });
//   await host.load(wasmBytes);
//   host.frame();              // call at host.frameRate Hz
//
// Platform concerns (canvas, GPU, audio device, input) are injected, so the same
// code runs the browser runner and the headless Node runner. `gfx` is a backend
// object (NullGfx here; WebGpuGfx in webgpu-gfx.js). Networking uses the global
// WebSocket and fetch(), which exist in browsers and Node >= 22. No dependencies.
//
// The implementation is split by concern under lib/; this module re-exports it all.

export { ABI_VERSION, CLIPBOARD_MAX, DEFAULT_MEMORY_LIMIT, FNV_INIT, GasmHost, memoryLimitMessage, ProcExit, STACK_SWITCHING, TITLE_MAX_BYTES, VirtualClock, cleanTitle, fnv32, staticTitle } from './lib/host.js';
export { Splitmix } from './lib/wasi.js';
export { BUF_COPY_DST, BUF_INDEX, BUF_STORAGE, BUF_UNIFORM, BUF_VERTEX, GfxModel, MAX_BIND_GROUPS, MAX_TEXTURE_SIZE, MAX_VERTEX_BUFFERS, NullGfx, OFFSET_ALIGNMENT, clampRect } from './lib/gfx.js';
export { MAX_CONNECTIONS, NET_CLOSED, NET_CONNECTING, NET_ERROR, NET_OPEN, NetConnections } from './lib/net.js';
export { SPLASH_FRAMES, SPLASH_H, SPLASH_HOLD, SPLASH_W, splashFrame, splashHash } from './lib/splash.js';
export { MANIFEST_SECTION, MANIFEST_VERSION, moduleManifest, parseManifest } from './lib/manifest.js';
export { FILES_FAILED, FILES_PENDING, FILES_SAVED, FileSaves, MAX_SAVE, safeName, validMime } from './lib/files.js';
export { Consent, FETCH_DONE, FETCH_FAILED, FETCH_HEADERS, FETCH_PENDING, FetchRequests, NetPolicy, recordKey } from './lib/fetch.js';
export {
  IdbStorage, MemoryStorage, STORAGE_ERR_IO, STORAGE_ERR_KEY, STORAGE_ERR_QUOTA, STORAGE_ERR_SIZE, STORAGE_MAX_VALUE, STORAGE_QUOTA,
  StorageError, validKey,
} from './lib/storage.js';
export {
  AssetTable, byCodePoint, bytesSource, directoryHandleAssets, directoryHandleEntries, fileAssets, fileListAssets, fileListEntries,
  isAssetProvider, memoryAssets, opfsAssets, preloadAssets,
} from './lib/assets.js';
export {
  BUTTONS, BrowserInput, DEFAULT_KEYMAP, GAMEPAD_AXES, GAMEPAD_BUTTONS, GAMEPAD_BYTES, INPUT_KEYS_RAW, INPUT_POINTER_HIDDEN,
  INPUT_POINTER_LOCKED, KEY_CODES, KEY_STATE_BYTES, POINTER_BYTES, browserGamepads, framePosition, letterbox, gamepadPads, keyCode,
  keyboardPads, normalizeCode, parseKeymap,
} from './lib/input.js';
export { Resampler } from './lib/audio.js';
