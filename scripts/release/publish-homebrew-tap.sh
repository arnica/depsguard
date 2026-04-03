#!/usr/bin/env bash
# Bump Formula/depsguard.rb in a Homebrew tap (binary release URLs + sha256).
set -euo pipefail

: "${GITHUB_REF:?}"
: "${GITHUB_REPOSITORY:?}"
: "${HOMEBREW_TAP_TOKEN:?}"
: "${HOMEBREW_TAP:?}"

TAG="${GITHUB_REF#refs/tags/}"
VERSION="${TAG#v}"
REPO_URL="https://github.com/${GITHUB_REPOSITORY}"
REPO_HOMEPAGE="${REPO_HOMEPAGE:-https://arnica.github.io/depsguard}"

mkdir -p /tmp/depsguard-sha
gh release download "$TAG" -R "$GITHUB_REPOSITORY" -p "*.sha256" -D /tmp/depsguard-sha

read_sha() {
  local name="$1"
  local f="/tmp/depsguard-sha/${name}.sha256"
  if [[ ! -f "$f" ]]; then
    echo "error: missing checksum file for ${name}" >&2
    exit 1
  fi
  awk '{print $1}' "$f"
}

SHA_AARCH64_APPLE_DARWIN="$(read_sha depsguard-aarch64-apple-darwin.tar.gz)"
SHA_X86_64_APPLE_DARWIN="$(read_sha depsguard-x86_64-apple-darwin.tar.gz)"
SHA_AARCH64_UNKNOWN_LINUX_GNU="$(read_sha depsguard-aarch64-unknown-linux-gnu.tar.gz)"
SHA_X86_64_UNKNOWN_LINUX_GNU="$(read_sha depsguard-x86_64-unknown-linux-gnu.tar.gz)"

export VERSION
export REPO_URL
export REPO_HOMEPAGE
export SHA_AARCH64_APPLE_DARWIN
export SHA_X86_64_APPLE_DARWIN
export SHA_AARCH64_UNKNOWN_LINUX_GNU
export SHA_X86_64_UNKNOWN_LINUX_GNU

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
envsubst <"${ROOT}/packaging/homebrew/depsguard.rb.in" >/tmp/depsguard.rb

git clone --depth 1 "https://x-access-token:${HOMEBREW_TAP_TOKEN}@github.com/${HOMEBREW_TAP}.git" /tmp/homebrew-tap
mkdir -p /tmp/homebrew-tap/Formula
cp /tmp/depsguard.rb /tmp/homebrew-tap/Formula/depsguard.rb
cd /tmp/homebrew-tap
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add Formula/depsguard.rb
if git diff --cached --quiet; then
  echo "Homebrew formula unchanged; nothing to commit."
  exit 0
fi
git commit -m "depsguard ${VERSION}

Synced from ${REPO_URL}/releases/tag/${TAG}"
git push origin HEAD
