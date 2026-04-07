#!/usr/bin/env bash
# Render bucket/depsguard.json from the Scoop manifest template.
# Called by CI after a GitHub Release is published.
set -euo pipefail

: "${GITHUB_REPOSITORY:?}"

TAG="${RELEASE_TAG:-${GITHUB_REF#refs/tags/}}"
VERSION="${TAG#v}"
REPO_URL="https://github.com/${GITHUB_REPOSITORY}"
GITHUB_REPO_SLUG="${GITHUB_REPOSITORY}"
REPO_HOMEPAGE="${REPO_HOMEPAGE:-https://depsguard.com}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

gh release download "$TAG" -R "$GITHUB_REPOSITORY" -p "depsguard-x86_64-pc-windows-msvc.zip.sha256" -D "${TMP_DIR}"

sha_file="${TMP_DIR}/depsguard-x86_64-pc-windows-msvc.zip.sha256"
if [[ ! -f "$sha_file" ]]; then
  echo "error: missing Windows zip checksum" >&2
  exit 1
fi
SHA_X86_64_PC_WINDOWS_MSVC_ZIP="$(awk '{print $1}' "$sha_file")"

export VERSION
export REPO_URL
export GITHUB_REPO_SLUG
export REPO_HOMEPAGE
export SHA_X86_64_PC_WINDOWS_MSVC_ZIP

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "${ROOT}/bucket"
envsubst <"${ROOT}/packaging/scoop/depsguard.json.in" >"${ROOT}/bucket/depsguard.json"
echo "Rendered bucket/depsguard.json for ${TAG}"
