/* gasm_manifest.h: embed the game's capabilities manifest (custom section
 * "gasm.manifest") in the .wasm. Once, in any one source file:
 *
 *   #include "gasm_manifest.h"
 *   GASM_MANIFEST({ "manifest": 1, "name": "My Game", "hosts": ["api.example.org"], "files": true });
 *
 * The JSON is written as is (no quoting); runners read it before the game starts:
 * "requires" (modules it can't run without), "hosts" (asked about once, up front),
 * "files" (saves for the player). See ABI.md, "Capabilities manifest". wasm-opt keeps
 * the section. */
#ifndef GASM_MANIFEST_H
#define GASM_MANIFEST_H

#define GASM__QUOTE(s) #s
#define GASM__MANIFEST(s) __asm__(".section .custom_section.gasm.manifest,\"\",@\n.ascii " GASM__QUOTE(s) "\n.text\n")
#define GASM_MANIFEST(...) GASM__MANIFEST(#__VA_ARGS__)

#endif
