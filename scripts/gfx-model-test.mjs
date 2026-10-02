#!/usr/bin/env node
// Replays tests/gfx-cases.json on the JS runner's gfx validation (GfxModel through
// GasmHost's gasm:gfx imports, null backend), as runners/native/src/gfx.rs's
// `shared_validation_cases` test does natively: both must accept and reject the
// same calls.
//   node scripts/gfx-model-test.mjs
import { readFileSync } from 'node:fs';
import { GasmHost } from '../runners/web/gasm-host.js';

const doc = JSON.parse(readFileSync(new URL('../tests/gfx-cases.json', import.meta.url)));
let pass = 0, fail = 0;
for (const c of doc.cases) {
  const host = new GasmHost({ onLog: () => {} });
  host.memory = new WebAssembly.Memory({ initial: 4 });
  const gfx = host.gfxImports();
  let at = 0;
  // strings and data go into guest memory, like a guest would pass them
  const put = (bytes) => { const p = at; host.bytes(p, bytes.length).set(bytes); at += (bytes.length + 7) & ~7; return [p, bytes.length]; };
  const str = (s) => put(new TextEncoder().encode(typeof s === 'string' ? s : JSON.stringify(s)));
  let failed = null;
  c.calls.forEach(([fn, ...a], i) => {
    if (failed !== null) return;
    try {
      at = 0;
      switch (fn) {
        case 'create_shader': gfx.create_shader(...str(doc.shader)); break;
        case 'create_pipeline': case 'create_bind_group': case 'create_bind_group_layout': case 'create_texture':
          gfx[fn](...str(a[0])); break;
        case 'write_buffer': { const [p, n] = put(new Uint8Array(a[2])); gfx.write_buffer(a[0], a[1], p, n); break; }
        case 'write_texture': { const [p, n] = put(new Uint8Array(a[6])); gfx.write_texture(a[0], a[1], a[2], a[3], a[4], a[5], p, n); break; }
        case 'begin_frame': gfx.begin_frame(0, 0, 0, 0); break;
        default: gfx[fn](...a);
      }
    } catch (e) { failed = [i, e.message]; }
  });
  const ok = (failed?.[0] ?? null) === c.fails;
  if (ok) pass++; else fail++;
  if (!ok) console.log(`FAIL  ${c.name}: expected ${c.fails === null ? 'success' : `call ${c.fails} to fail`}, got ${failed ? `call ${failed[0]}: ${failed[1]}` : 'success'}`);
}
console.log(`gfx validation cases: ${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
