#!/usr/bin/env bash
# Update depsguard formula in a Homebrew tap repository.
set -euo pipefail

: "${GITHUB_REF:?}"
: "${GITHUB_REPOSITORY:?}"
: "${HOMEBREW_TAP_TOKEN:?}"

TAG="${GITHUB_REF#refs/tags/}"
VERSION="${TAG#v}"
REPO_URL="https://github.com/${GITHUB_REPOSITORY}"
SOURCE_URL="${REPO_URL}/archive/refs/tags/v${VERSION}.tar.gz"

TAP_REPO="${HOMEBREW_TAP_REPO:-${GITHUB_REPOSITORY}}"
if [[ -z "${TAP_REPO}" || "${TAP_REPO}" != */* ]]; then
  echo "error: HOMEBREW_TAP_REPO must be in owner/repo format (got '${TAP_REPO}')" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

curl -fsSL \
  -H "Authorization: Bearer ${HOMEBREW_TAP_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  "${SOURCE_URL}" \
  -o "${TMP_DIR}/depsguard-source.tar.gz"
SHA256_SOURCE="$(sha256sum "${TMP_DIR}/depsguard-source.tar.gz" | awk '{print $1}')"
export VERSION GITHUB_REPOSITORY SHA256_SOURCE

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
envsubst <"${ROOT}/packaging/homebrew/depsguard.rb.in" >"${TMP_DIR}/depsguard.rb"

AUTH_HEADER="AUTHORIZATION: basic $(printf 'x-access-token:%s' "${HOMEBREW_TAP_TOKEN}" | base64 | tr -d '\n')"
git -c "http.https://github.com/.extraheader=${AUTH_HEADER}" \
  clone --depth 1 "https://github.com/${TAP_REPO}.git" "${TMP_DIR}/homebrew-tap"
cd "${TMP_DIR}/homebrew-tap"

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

mkdir -p Formula
cp "${TMP_DIR}/depsguard.rb" Formula/depsguard.rb
git add -f Formula/depsguard.rb

if git diff --cached --quiet; then
  echo "Homebrew tap formula unchanged; nothing to commit."
  exit 0
fi

git commit -m "depsguard ${VERSION}

Synced from ${REPO_URL}/releases/tag/${TAG}"
git -c "http.https://github.com/.extraheader=${AUTH_HEADER}" push origin HEAD
echo "Updated ${TAP_REPO}/Formula/depsguard.rb for ${VERSION}."
