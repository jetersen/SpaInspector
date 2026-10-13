#!/usr/bin/env bash
set -euo pipefail
# Official Linux x64 release, pinned to the publisher's SHA-256 checksum.
archive="$(mktemp)"
trap 'rm -f "$archive"' EXIT
curl -fsSL https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-musl.tar.gz -o "$archive"
echo "a67f4054b249fe9acc5fabc25de1aebf19783aca3ad6ff64bf34d7da44d0ea20  $archive" | sha256sum -c -
mkdir -p "$HOME/.cargo/bin"
tar -xzf "$archive" -C "$HOME/.cargo/bin" trunk
trunk --version
