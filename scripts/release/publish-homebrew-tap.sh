#!/usr/bin/env bash
# Update depsguard formula in a first-party Homebrew tap repository.
set -euo pipefail

: "${GITHUB_REF:?}"
: "${GITHUB_REPOSITORY:?}"
: "${HOMEBREW_TAP_TOKEN:?}"

TAG="${GITHUB_REF#refs/tags/}"
VERSION="${TAG#v}"
REPO_URL="https://github.com/${GITHUB_REPOSITORY}"

REPO_OWNER="${GITHUB_REPOSITORY%%/*}"
TAP_REPO="${HOMEBREW_TAP_REPO:-${GITHUB_REPOSITORY_OWNER:-${REPO_OWNER}}/homebrew-depsguard}"
if [[ -z "${TAP_REPO}" || "${TAP_REPO}" != */* ]]; then
  echo "error: HOMEBREW_TAP_REPO must be in owner/repo format (got '${TAP_REPO}')" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

curl -fsSL \
  -H "Authorization: Bearer ${HOMEBREW_TAP_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/${GITHUB_REPOSITORY}/tarball/${TAG}" \
  -o "${TMP_DIR}/depsguard-source.tar.gz"
SHA256_SOURCE="$(sha256sum "${TMP_DIR}/depsguard-source.tar.gz" | awk '{print $1}')"
export VERSION GITHUB_REPOSITORY SHA256_SOURCE

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
envsubst <"${ROOT}/packaging/homebrew/depsguard.rb.in" >"${TMP_DIR}/depsguard.rb"

git clone --depth 1 "https://x-access-token:${HOMEBREW_TAP_TOKEN}@github.com/${TAP_REPO}.git" "${TMP_DIR}/homebrew-tap"
cd "${TMP_DIR}/homebrew-tap"

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

mkdir -p Formula
cp "${TMP_DIR}/depsguard.rb" Formula/depsguard.rb
git add Formula/depsguard.rb

if git diff --cached --quiet; then
  echo "Homebrew tap formula unchanged; nothing to commit."
  exit 0
fi

git commit -m "depsguard ${VERSION}

Synced from ${REPO_URL}/releases/tag/${TAG}"
git push origin HEAD
echo "Updated ${TAP_REPO}/Formula/depsguard.rb for ${VERSION}."
