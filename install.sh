#!/usr/bin/env bash
# Install difforge from this repo into ~/.cargo/bin, replacing any older copy.
set -euo pipefail

root=$(cd "$(dirname "$0")" && pwd)
cd "$root"

if [[ ! -f Cargo.toml ]] || ! grep -q '^name = "difforge"' Cargo.toml; then
  echo "install.sh: run this from the DiffForge repo (missing Cargo.toml)." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  if [[ -f "${CARGO_HOME:-$HOME/.cargo}/env" ]]; then
    # shellcheck disable=SC1090
    . "${CARGO_HOME:-$HOME/.cargo}/env"
  fi
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Installing Rust via rustup…"
  if ! command -v curl >/dev/null 2>&1; then
    echo "install.sh: cargo is missing and so is curl." >&2
    echo "Install Rust from https://rustup.rs then run this script again." >&2
    exit 1
  fi
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1090
  . "${CARGO_HOME:-$HOME/.cargo}/env"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "install.sh: cargo still not on PATH after rustup." >&2
  echo "Open a new shell or: . \"${CARGO_HOME:-$HOME/.cargo}/env\"" >&2
  exit 1
fi

echo "Installing difforge (replacing any existing binary)…"
cargo install --path . --force --locked
echo "Installed $(command -v difforge)"
echo "Need git and difft on PATH. Launch with: difforge"
