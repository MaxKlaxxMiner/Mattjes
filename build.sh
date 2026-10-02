#!/usr/bin/env bash
# Builds mattjesGo and/or mattjesRs and places the binaries in the repo root.
#
#   ./build.sh        # both
#   ./build.sh go     # Go only   -> runMattjesGo.exe
#   ./build.sh rs     # Rust only -> runMattjesRs.exe
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
what="${1:-all}"

if [[ "$what" == "all" || "$what" == "go" ]]; then
  echo "[go]   building runMattjesGo.exe"
  (cd "$root/mattjesGo" && go build -o "$root/runMattjesGo.exe" .)
fi

if [[ "$what" == "all" || "$what" == "rs" ]]; then
  echo "[rust] building runMattjesRs.exe"
  (cd "$root/mattjesRs" && cargo build --release --quiet)
  cp "$root/mattjesRs/target/release/mattjes.exe" "$root/runMattjesRs.exe"
fi

echo "done."
