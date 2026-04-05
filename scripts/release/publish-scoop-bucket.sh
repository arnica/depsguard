#!/usr/bin/env bash
# Update depsguard.json in a Scoop bucket repository (Windows zip + sha256).
set -euo pipefail

: "${GITHUB_REF:?}"
: "${GITHUB_REPOSITORY:?}"
: "${SCOOP_BUCKET_TOKEN:?}"
: "${SCOOP_BUCKET:?}"

TAG="${GITHUB_REF#refs/tags/}"
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
envsubst <"${ROOT}/packaging/scoop/depsguard.json.in" >"${TMP_DIR}/depsguard.json"

AUTH_HEADER="AUTHORIZATION: basic $(printf 'x-access-token:%s' "${SCOOP_BUCKET_TOKEN}" | base64 | tr -d '\n')"
git -c "http.https://github.com/.extraheader=${AUTH_HEADER}" \
  clone --depth 1 "https://github.com/${SCOOP_BUCKET}.git" "${TMP_DIR}/scoop-bucket"
cp "${TMP_DIR}/depsguard.json" "${TMP_DIR}/scoop-bucket/depsguard.json"
cd "${TMP_DIR}/scoop-bucket"
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add depsguard.json
if git diff --cached --quiet; then
  echo "Scoop manifest unchanged; nothing to commit."
  exit 0
fi
git commit -m "depsguard: ${VERSION}

Synced from ${REPO_URL}/releases/tag/${TAG}"
git -c "http.https://github.com/.extraheader=${AUTH_HEADER}" push origin HEAD
