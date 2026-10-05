// gasm:files: files a game hands the player (runners/native/src/files.rs does the same
// natively). `save` queues a copy; after the frame the host hands each to the embedder's
// onSaveFile(name, mime, bytes) (the player: a download; gasm-headless: --save-dir),
// which returns true/false or a promise of it. Same refusals and states as natively.

export const FILES_PENDING = 0;
export const FILES_SAVED = 1;
export const FILES_FAILED = 2;
/** The largest file a game can save */
export const MAX_SAVE = 256 << 20;
/** Saves queued and not yet handed over, at most */
export const MAX_PENDING_SAVES = 16;

const MIME_PART = /^[A-Za-z0-9!#$&^_.+-]{1,127}$/;
/** `type/subtype` with RFC 6838 name characters. */
export function validMime(m) {
  const i = m.indexOf('/');
  return i > 0 && MIME_PART.test(m.slice(0, i)) && MIME_PART.test(m.slice(i + 1));
}

/** The last path component, safe as a file name (as natively; downloads get it as their name). */
export function safeName(name) {
  let s = name.split(/[/\\]/).pop().replace(/[\u0000-\u001f\u007f-\u009f<>:"|?*]/g, '_');
  s = s.replace(/^[. ]+/, '').replace(/[. ]+$/, '');
  if (!s) return 'file';
  const stem = s.split('.')[0].toUpperCase();
  return /^(CON|PRN|AUX|NUL|COM\d|LPT\d)$/.test(stem) ? `_${s}` : s;
}

export class FileSaves {
  /** onSave(name, mime, bytes) -> boolean | Promise<boolean>; null: saving is off. */
  constructor(onSave, log) {
    this.onSave = onSave;
    this.log = log;
    this.states = [];
    this.queue = [];
  }

  save(name, nameBytes, mime, data) {
    const why = !this.onSave ? 'saving is turned off'
      : nameBytes === 0 || nameBytes > 255 ? 'the name must be 1 to 255 bytes'
      : !validMime(mime) ? 'the type must be type/subtype (image/png)'
      : data.length > MAX_SAVE ? 'over 256 MiB'
      : this.queue.length >= MAX_PENDING_SAVES ? '16 saves are still pending'
      : null;
    if (why) { this.log(`[gasm] files: save ${JSON.stringify(name)} refused: ${why}`); return -1; }
    this.states.push(FILES_PENDING);
    this.queue.push([this.states.length - 1, name, mime, data.slice()]);
    return this.states.length;
  }

  /** The state of a save; throws (traps) for a handle `save` never returned. */
  state(h) {
    if (!Number.isInteger(h) || h <= 0 || h > this.states.length) throw new Error(`gasm:files: invalid handle ${h}`);
    return this.states[h - 1];
  }

  /** Hand the frame's saves to the embedder (between frames). */
  flush() {
    for (const [i, name, mime, data] of this.queue.splice(0)) {
      const done = (ok) => { this.states[i] = ok ? FILES_SAVED : FILES_FAILED; };
      try {
        const r = this.onSave(safeName(name), mime, data);
        if (r && typeof r.then === 'function') r.then((ok) => done(ok !== false), (e) => { this.log(`[gasm] files: save ${JSON.stringify(name)} failed: ${e.message}`); done(false); });
        else done(r !== false);
      } catch (e) { this.log(`[gasm] files: save ${JSON.stringify(name)} failed: ${e.message}`); done(false); }
    }
  }
}
