#!/bin/sh
# Build the local NEX test server (tools/nex-local) into core/target/nex-local/nex-local.
#
# It links Nextendo Network's public Go server core, nextendo-nex (PolyForm Shield 1.0.0), which
# is never copied into this repository: give a local clone with NEXTENDO_NEX_SRC, or let Go fetch
# github.com/NextendoNetwork/nextendo-nex. The binary stays under core/target/ (git-ignored).
#
#   scripts/nex-local.sh            build (needs Go 1.23+)
#   scripts/nex-local.sh --run ...  build, then run it with the given flags
#
# The Rust integration tests (core/crates/nex/tests/local_server.rs) use the binary when it is
# there, or $SMCRAFT_NEX_LOCAL, and skip otherwise.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/core/target/nex-local"
src=${NEXTENDO_NEX_SRC:-/Volumes/SSD/smm-work/nextendo/nextendo-nex}
command -v go >/dev/null || { echo "nex-local: Go is not installed (brew install go)" >&2; exit 1; }
mkdir -p "$out/build"
cp "$root/tools/nex-local/main.go" "$out/build/main.go"
cd "$out/build"
rm -f go.mod go.sum
go mod init smcraft.local/nex-local >/dev/null 2>&1
if [ -d "$src" ]; then
  go mod edit -require=github.com/NextendoNetwork/nextendo-nex@v0.0.0 \
    -replace=github.com/NextendoNetwork/nextendo-nex="$src"
fi
go mod tidy >/dev/null
go build -o "$out/nex-local" .
echo "nex-local: built $out/nex-local"
if [ "$1" = "--run" ]; then
  shift
  exec "$out/nex-local" "$@"
fi
