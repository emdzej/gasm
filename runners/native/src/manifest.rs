//! The capabilities manifest: what a game (or a mod) says it needs, as JSON. Games
//! embed it as the custom section `gasm.manifest` (Rust: `gasm::manifest!`, C:
//! `GASM_MANIFEST`); a launcher can give it instead (asset `gasm.manifest`, or
//! `gasm-run --manifest <file>`: Godot games share one engine module). Mods bring
//! theirs next to the pack (`roads.pck` + `roads.json`).
//!
//! ```json
//! { "manifest": 1, "name": "Nowhere in Particular", "requires": ["gasm:gl"],
//!   "hosts": ["api.met.no", "*.example.org"], "files": true }
//! ```
//!
//! - `requires`: modules (or `module.function`) the game can't run without; a runner
//!   lacking one refuses it before it starts.
//! - `hosts`: network hosts it will reach; the player is asked once, up front, instead
//!   of host by host (hosts it didn't declare are still asked about when reached).
//! - `files`: it saves files for the player (gasm:files); asked up front too.
//!
//! Unknown fields are ignored (later versions add some); a manifest that isn't valid
//! JSON or has a newer `manifest` version is refused with the reason.

use serde_json::Value;

pub const SECTION: &str = "gasm.manifest";
pub const VERSION: u64 = 1;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Manifest {
    pub name: Option<String>,
    /// the game's id: its saves' namespace (and consent, default folders) instead of
    /// the module's file name; `[A-Za-z0-9._-]`, 1 to 128 bytes. Only honoured from a
    /// manifest the launcher gives (runners choose namespaces, never the guest)
    pub id: Option<String>,
    /// the window's icon: the name of a PNG asset (`gasm-run --icon` overrides it)
    pub icon: Option<String>,
    pub requires: Vec<String>,
    pub hosts: Vec<String>,
    pub files: bool,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
        let o = v.as_object().ok_or("not a JSON object")?;
        match o.get("manifest").and_then(Value::as_u64) {
            Some(n) if n <= VERSION => {}
            Some(n) => return Err(format!("manifest version {n}: this runner knows {VERSION}")),
            None => return Err("no \"manifest\": 1 field".into()),
        }
        let strings = |k: &str| -> Result<Vec<String>, String> {
            match o.get(k) {
                None => Ok(Vec::new()),
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|x| x.as_str().map(str::to_owned).ok_or_else(|| format!("\"{k}\" must be a list of strings")))
                    .collect(),
                Some(_) => Err(format!("\"{k}\" must be a list of strings")),
            }
        };
        let hosts: Vec<String> = strings("hosts")?.into_iter().map(|h| h.trim().trim_end_matches('.').to_ascii_lowercase()).collect();
        if let Some(h) = hosts.iter().find(|h| h.is_empty() || h.contains(['/', ':', ' ', ','])) {
            return Err(format!("{h:?} is not a host name (api.example.org, *.example.org)"));
        }
        let id = match o.get("id") {
            None => None,
            Some(Value::String(s)) if crate::storage::valid_key(s) => Some(s.clone()),
            Some(_) => return Err("\"id\" must be 1 to 128 characters of A-Z a-z 0-9 . _ -".into()),
        };
        Ok(Manifest {
            name: o.get("name").and_then(Value::as_str).map(str::to_owned),
            id,
            icon: o.get("icon").and_then(Value::as_str).map(str::to_owned),
            requires: strings("requires")?,
            hosts,
            files: o.get("files").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    /// The requirements this runner can't meet (`provided`: what `gasm.has` answers yes to).
    pub fn missing(&self, provided: &std::collections::HashSet<String>) -> Vec<String> {
        self.requires.iter().filter(|r| !provided.contains(r.as_str())).cloned().collect()
    }
}

/// The text of a module's `gasm.manifest` custom section, if it has one (a .cwasm has none).
pub fn from_module(wasm: &[u8]) -> Option<Result<String, String>> {
    let body = wasm.strip_prefix(b"\0asm\x01\0\0\0")?;
    let mut p = 0;
    let leb = |p: &mut usize| -> Option<usize> {
        let (mut v, mut shift) = (0usize, 0);
        loop {
            let b = *body.get(*p)?;
            *p += 1;
            v |= ((b & 0x7f) as usize) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 35 {
                return None;
            }
        }
    };
    while p < body.len() {
        let id = body[p];
        p += 1;
        let size = leb(&mut p)?;
        let end = p.checked_add(size).filter(|&e| e <= body.len())?;
        if id == 0 {
            let mut q = p;
            let n = leb(&mut q)?;
            let name = body.get(q..q.checked_add(n)?)?;
            if name == SECTION.as_bytes() {
                let data = &body[q + n..end];
                return Some(std::str::from_utf8(data).map(str::to_owned).map_err(|_| "the gasm.manifest section is not UTF-8".to_string()));
            }
        }
        p = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let m = Manifest::parse(r#"{"manifest":1,"name":"x","requires":["gasm:gl"],"hosts":["API.met.no."],"files":true,"later":5}"#).unwrap();
        assert_eq!(m, Manifest { name: Some("x".into()), id: None, icon: None, requires: vec!["gasm:gl".into()], hosts: vec!["api.met.no".into()], files: true });
        assert!(Manifest::parse(r#"{"manifest":2}"#).unwrap_err().contains("version 2"));
        assert!(Manifest::parse(r#"{"hosts":[]}"#).is_err());
        assert!(Manifest::parse(r#"{"manifest":1,"hosts":["http://x.org"]}"#).is_err());
        assert!(Manifest::parse("nope").is_err());
        assert_eq!(Manifest::parse(r#"{"manifest":1,"id":"nip"}"#).unwrap().id.as_deref(), Some("nip"));
        assert!(Manifest::parse(r#"{"manifest":1,"id":"a/b"}"#).is_err());
    }

    #[test]
    fn custom_section() {
        // a module with one custom section "gasm.manifest" holding "{}"
        let mut m = b"\0asm\x01\0\0\0".to_vec();
        let name = SECTION.as_bytes();
        let payload: Vec<u8> = [&[name.len() as u8][..], name, b"{}"].concat();
        m.push(0);
        m.push(payload.len() as u8);
        m.extend(payload);
        assert_eq!(from_module(&m), Some(Ok("{}".into())));
        assert_eq!(from_module(b"\0asm\x01\0\0\0"), None);
    }
}
