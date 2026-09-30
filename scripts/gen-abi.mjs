#!/usr/bin/env node
// Generate ABI bindings from spec/abi.json, and check runner conformance.
//
//   node scripts/gen-abi.mjs           write spec/gasm.h and guests/gasm/src/sys.rs
//   node scripts/gen-abi.mjs --check   fail if generated files are stale, or if a
//                                      runner / the native stub doesn't implement
//                                      exactly the functions in abi.json
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => readFileSync(join(ROOT, p), 'utf8');
const abi = JSON.parse(read('spec/abi.json'));
const T = abi.types;
const gasmModules = abi.modules.filter((m) => !m.wasi);

// ---- C header ------------------------------------------------------------------

function cParams(fn) {
  const ps = (fn.params ?? []).map((p) => {
    if (p.type === 'str') return T.str.c.replaceAll('{name}', p.name);
    if (p.type === 'ptr') return `${p.c}${p.c.endsWith('*') ? '' : ' '}${p.name}`;
    return `${T[p.type].c} ${p.name}`;
  });
  return ps.length ? ps.join(', ') : 'void';
}

function wrapComment(text, indent = '') {
  const words = text.split(/\s+/), lines = [];
  let line = '';
  for (const w of words) {
    if ((line + ' ' + w).trim().length > 76 - indent.length) { lines.push(line.trim()); line = ''; }
    line += ' ' + w;
  }
  if (line.trim()) lines.push(line.trim());
  return lines.length === 1 ? `${indent}/* ${lines[0]} */` : `${indent}/* ${lines.join(`\n${indent} * `)} */`;
}

function genHeader() {
  const out = [];
  out.push(`/*
 * gasm.h — gasm ABI v${abi.abi_version} (guest side, C/C++)
 *
 * GENERATED from spec/abi.json by scripts/gen-abi.mjs. Do not edit by hand.
 * Normative prose: spec/ABI.md · https://gasm.emdzej.pl/docs/abi
 *
 * A gasm game is a wasm32 module that imports the functions below (modules
 * "gasm", and optionally "gasm:gfx", "gasm:net", "gasm:storage") and exports:
${abi.exports.map((e) => ` *   ${e.name.padEnd(18)} ${e.required ? 'required' : 'optional'}${e.doc ? '  ' + e.doc : ''}`).join('\n')}
 *
 * All pointers are offsets into the guest's linear memory. Strings are UTF-8
 * (ptr, len), not NUL-terminated. Games using wasi-libc may also import a
 * WASI preview1 subset; there is no filesystem.
 */
#ifndef GASM_H
#define GASM_H

#include <stdint.h>

#define GASM_ABI_VERSION ${abi.abi_version}

#ifdef __wasm__
${gasmModules.map((m) => `#define ${m.c_macro}(name) __attribute__((import_module("${m.name}"), import_name(name)))`).join('\n')}
#define GASM_EXPORT(name) __attribute__((export_name(name)))
#else
${gasmModules.map((m) => `#define ${m.c_macro}(name)`).join('\n')}
#define GASM_EXPORT(name)
#endif

#ifdef __cplusplus
extern "C" {
#endif
`);
  for (const c of abi.constants) {
    out.push(`/* ---- ${c.group} ${'-'.repeat(Math.max(4, 72 - c.group.length))} */`);
    if (c.doc) out.push(wrapComment(c.doc));
    out.push('enum {');
    for (const [k, v] of c.values) out.push(`    ${k} = ${v},`);
    out.push('};\n');
  }
  for (const m of gasmModules) {
    out.push(`/* ---- ${m.name}${m.optional ? ' (optional)' : ''} ${'-'.repeat(Math.max(4, 66 - m.name.length))} */`);
    if (m.doc && m.name !== 'gasm') out.push(wrapComment(m.doc));
    out.push('');
    for (const f of m.functions) {
      if (f.doc) out.push(wrapComment(f.doc));
      const ret = f.result ? T[f.result].c : 'void';
      out.push(`${m.c_macro}("${f.name}") ${ret} ${m.c_prefix}${f.name}(${cParams(f)});`);
    }
    out.push('');
  }
  out.push(`#ifdef __cplusplus
}
#endif

/* ---- convenience (hand-written template in scripts/gen-abi.mjs) ---------- */

/* NUL-terminated string helpers; no libc needed (work in freestanding builds). */
static inline uint32_t gasm__strlen(const char *s) {
    uint32_t n = 0;
    while (s[n]) n++;
    return n;
}
static inline void gasm_log_str(const char *s) { gasm_log(s, gasm__strlen(s)); }
static inline int32_t gasm_asset_size_str(const char *n) { return gasm_asset_size(n, gasm__strlen(n)); }
static inline int32_t gasm_asset_read_str(const char *n, void *dst, uint32_t cap) {
    return gasm_asset_read(n, gasm__strlen(n), dst, cap);
}
/* Copy parameter \`name\` into \`dst\` as a NUL-terminated string; returns 0 if unset or too long. */
static inline int gasm_param_str(const char *name, char *dst, uint32_t cap) {
    if (cap == 0) return 0;
    int32_t n = gasm_param(name, gasm__strlen(name), dst, cap - 1);
    if (n < 0 || (uint32_t)n > cap - 1) { dst[0] = 0; return 0; }
    dst[n] = 0;
    return 1;
}
static inline uint32_t gasm_gfx_create_shader_str(const char *wgsl) {
    return gasm_gfx_create_shader(wgsl, gasm__strlen(wgsl));
}
static inline uint32_t gasm_gfx_create_pipeline_str(const char *json) {
    return gasm_gfx_create_pipeline(json, gasm__strlen(json));
}
static inline uint32_t gasm_gfx_create_bind_group_str(const char *json) {
    return gasm_gfx_create_bind_group(json, gasm__strlen(json));
}
static inline int32_t gasm_net_open_str(const char *url) { return gasm_net_open(url, gasm__strlen(url)); }

#endif /* GASM_H */
`);
  return out.join('\n');
}

// ---- Rust raw imports -------------------------------------------------------------

const rustName = (m, f) => `${m.rust_prefix}${f.name}`;

function rustParams(fn) {
  return (fn.params ?? []).map((p) => {
    if (p.type === 'str') return T.str.rust.replaceAll('{name}', p.name);
    if (p.type === 'ptr') return `${p.name}: ${p.rust}`;
    return `${p.name}: ${T[p.type].rust}`;
  }).join(', ');
}

function genRust() {
  const out = [`//! Raw ABI. On wasm32 these are the real imports (see spec/ABI.md). On other
//! targets they are backed by [\`crate::native\`], an in-process stub host used
//! for native parity tests and benchmarks.
//!
//! GENERATED from spec/abi.json by scripts/gen-abi.mjs. Do not edit by hand.

#![allow(clippy::missing_safety_doc)]

#[cfg(target_arch = "wasm32")]
mod imports {`];
  for (const m of abi.modules) {
    out.push(`    #[link(wasm_import_module = "${m.name}")]`, '    unsafe extern "C" {');
    for (const f of m.functions) {
      if (f.doc) out.push(`        /// ${f.doc}`);
      if (m.rust_prefix) out.push(`        #[link_name = "${f.name}"]`);
      const ret = f.noreturn ? ' -> !' : f.result ? ` -> ${T[f.result].rust}` : '';
      out.push(`        pub fn ${rustName(m, f)}(${rustParams(f)})${ret};`);
    }
    out.push('    }', '');
  }
  out[out.length - 1] = '}';
  out.push(`
#[cfg(target_arch = "wasm32")]
pub use imports::*;

#[cfg(not(target_arch = "wasm32"))]
pub use crate::native::abi::*;
`);
  return out.join('\n');
}

// ---- conformance ---------------------------------------------------------------------

function section(text, start, end) {
  const i = text.indexOf(start);
  if (i < 0) return '';
  const j = text.indexOf(end, i + start.length);
  return text.slice(i, j < 0 ? undefined : j);
}

function conformance() {
  const errors = [];
  const compare = (where, want, have) => {
    for (const n of want) if (!have.has(n)) errors.push(`${where}: missing ${n}`);
    for (const n of have) if (!want.has(n)) errors.push(`${where}: not in abi.json: ${n}`);
  };
  const hostRs = read('runners/native/src/host.rs');
  const hostJs = read('runners/web/gasm-host.js');
  const nativeRs = read('guests/gasm/src/native.rs');
  const jsMethod = { gasm: 'gasmImports()', 'gasm:gfx': 'gfxImports()', 'gasm:net': 'netImports()', 'gasm:storage': 'storageImports()' };

  for (const m of gasmModules) {
    const want = new Set(m.functions.map((f) => f.name));
    // native runner (wasmtime): func_wrap("gasm", "name") or func_wrap(M, "name") after const M
    const rs = m.name === 'gasm'
      ? [...hostRs.matchAll(/func_wrap\(\s*"gasm",\s*"([a-z_]+)"/g)].map((x) => x[1])
      : [...section(hostRs, `const M: &str = "${m.name}";`, '\nfn ').matchAll(/func_wrap\(\s*M,\s*"([a-z_]+)"/g)].map((x) => x[1]);
    compare(`runners/native (${m.name})`, want, new Set(rs));
    // JS runner: keys of the import object returned by the matching method
    const js = [...section(hostJs, jsMethod[m.name], '\n  }\n').matchAll(/^ {6}([a-z_]+): /gm)].map((x) => x[1]);
    compare(`runners/web (${m.name})`, want, new Set(js));
  }
  // native stub host in the Rust SDK implements every function (incl. proc_exit)
  const stub = new Set([...section(nativeRs, 'pub mod abi', '\n}\n').matchAll(/pub unsafe fn ([a-z_0-9]+)\(/g)].map((x) => x[1]));
  const wantStub = new Set(abi.modules.flatMap((m) => m.functions.map((f) => rustName(m, f))));
  compare('guests/gasm native stub', wantStub, stub);
  if (!/proc_exit:/.test(hostJs)) errors.push('runners/web: WASI proc_exit missing');
  return errors;
}

// The default keyboard layout is shared: runners/native/src/default-keymap.txt must equal
// gasm-host.js DEFAULT_KEYMAP (same text format, same bindings on both runners).
async function keymapCheck() {
  const { DEFAULT_KEYMAP } = await import(join(ROOT, 'runners/web/gasm-host.js'));
  return read('runners/native/src/default-keymap.txt') === DEFAULT_KEYMAP
    ? [] : ['runners/native/src/default-keymap.txt differs from gasm-host.js DEFAULT_KEYMAP'];
}

// ---- main ----------------------------------------------------------------------------------

const outputs = { 'spec/gasm.h': genHeader(), 'guests/gasm/src/sys.rs': genRust() };
if (process.argv.includes('--check')) {
  const errors = [...conformance(), ...(await keymapCheck())];
  for (const [p, content] of Object.entries(outputs)) {
    if (read(p) !== content) errors.push(`${p} is out of date: run node scripts/gen-abi.mjs`);
  }
  if (errors.length) {
    console.error('ABI check failed:\n  ' + errors.join('\n  '));
    process.exit(1);
  }
  const n = abi.modules.reduce((s, m) => s + m.functions.length, 0);
  console.log(`ABI check ok: ${n} functions in ${abi.modules.length} modules; header, Rust bindings, both runners and the native stub agree`);
} else {
  for (const [p, content] of Object.entries(outputs)) writeFileSync(join(ROOT, p), content);
  console.log(`generated ${Object.keys(outputs).join(', ')}`);
}
