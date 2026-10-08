#!/usr/bin/env bash
# Builds mattjesGo and/or mattjesRs and places the binaries in the repo root.
#
#   ./build.sh        # both
#   ./build.sh go     # Go only   -> runMattjesGo.exe
#   ./build.sh rs     # Rust only -> runMattjesRs.exe
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
what="${1:-all}"

# Replaces a binary that may be running (a chess GUI keeps it open): Windows
# refuses to overwrite a running exe but allows renaming it, so the old file is
# moved aside first (what "go build -o" does internally). Leftover .old files
# are removed on the next build, once nothing holds them.
install_exe() {
  local src="$1" dst="$2"
  rm -f "$dst.old" 2>/dev/null || true
  if [[ -e "$dst" ]]; then
    mv -f "$dst" "$dst.old"
  fi
  cp "$src" "$dst"
  rm -f "$dst.old" 2>/dev/null || true
}

if [[ "$what" == "all" || "$what" == "go" ]]; then
  echo "[go]   building runMattjesGo.exe"
  (cd "$root/mattjesGo" && go build -o "$root/runMattjesGo.exe" .)
fi

if [[ "$what" == "all" || "$what" == "rs" ]]; then
  echo "[rust] building runMattjesRs.exe"
  (cd "$root/mattjesRs" && cargo build --release --quiet)
  install_exe "$root/mattjesRs/target/release/mattjes.exe" "$root/runMattjesRs.exe"
fi

echo "done."
