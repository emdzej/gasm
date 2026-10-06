// The capabilities manifest (runners/native/src/manifest.rs does the same natively):
// what a game or a mod needs, as JSON. Games embed it as the custom section
// "gasm.manifest"; launchers can give it as asset "gasm.manifest" (Godot games share
// one engine module); mods bring "<stem>.json" next to the pack.
//   { "manifest": 1, "name": "…", "requires": ["gasm:gl"], "hosts": ["api.met.no"], "files": true }

export const MANIFEST_SECTION = 'gasm.manifest';
export const MANIFEST_VERSION = 1;

/** Parse a manifest: { name, requires, hosts, files }; throws why it's refused. */
export function parseManifest(text) {
  let v;
  try { v = JSON.parse(text); } catch (e) { throw new Error(`not valid JSON: ${e.message}`); }
  if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('not a JSON object');
  if (!Number.isInteger(v.manifest)) throw new Error('no "manifest": 1 field');
  if (v.manifest > MANIFEST_VERSION) throw new Error(`manifest version ${v.manifest}: this runner knows ${MANIFEST_VERSION}`);
  const strings = (k) => {
    if (v[k] === undefined) return [];
    if (!Array.isArray(v[k]) || v[k].some((x) => typeof x !== 'string')) throw new Error(`"${k}" must be a list of strings`);
    return v[k];
  };
  const hosts = strings('hosts').map((h) => h.trim().replace(/\.+$/, '').toLowerCase());
  const bad = hosts.find((h) => !h || /[/: ,]/.test(h));
  if (bad !== undefined) throw new Error(`${JSON.stringify(bad)} is not a host name (api.example.org, *.example.org)`);
  return { name: typeof v.name === 'string' ? v.name : null, requires: strings('requires'), hosts, files: v.files === true };
}

/** The text of a compiled module's gasm.manifest section, or null. */
export function moduleManifest(module) {
  const s = WebAssembly.Module.customSections(module, MANIFEST_SECTION);
  if (!s.length) return null;
  try { return new TextDecoder('utf-8', { fatal: true }).decode(s[0]); } catch { throw new Error('the gasm.manifest section is not UTF-8'); }
}
