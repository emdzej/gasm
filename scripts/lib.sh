# Shared helpers for the fetch/build scripts (sourced; bash 3.2-safe).

# sha256_of <file>: print the file's SHA-256 (shasum on macOS, sha256sum elsewhere).
sha256_of() {
  local s
  s=$(shasum -a 256 "$1" 2>/dev/null || sha256sum "$1")
  echo "${s%% *}"
}

# sha256_ok <file> <sha256>: fail (exit 1) unless the file has that SHA-256.
sha256_ok() {
  local got
  got=$(sha256_of "$1")
  [ "$got" = "$2" ] || { echo "$1: checksum mismatch (got $got, want $2)" >&2; exit 1; }
}

# download <url> <out> <sha256>: fetch to <out> and verify it.
download() {
  curl -fsSL "$1" -o "$2"
  sha256_ok "$2" "$3"
}
