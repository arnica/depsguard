#!/usr/bin/env bash
# Open or update Homebrew/homebrew-core PR for Formula/d/depsguard.rb.
set -euo pipefail

: "${GITHUB_REF:?}"
: "${GITHUB_REPOSITORY:?}"
: "${HOMEBREW_CORE_TOKEN:?}"

TAG="${GITHUB_REF#refs/tags/}"
VERSION="${TAG#v}"

BASE_REPO="Homebrew/homebrew-core"
CORE_FORK="${HOMEBREW_CORE_FORK:-}"
if [[ -z "${CORE_FORK}" ]]; then
  CORE_FORK="${GITHUB_ACTOR}/homebrew-core"
fi

FORK_OWNER="${CORE_FORK%%/*}"
if [[ "${FORK_OWNER}" == "${CORE_FORK}" ]]; then
  echo "error: HOMEBREW_CORE_FORK must be in owner/repo format (got '${CORE_FORK}')" >&2
  exit 1
fi

BRANCH="depsguard-${VERSION}"
SOURCE_URL="https://github.com/${GITHUB_REPOSITORY}/archive/refs/tags/${TAG}.tar.gz"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

curl -fsSL "${SOURCE_URL}" -o "${TMP_DIR}/depsguard-source.tar.gz"
SHA256_SOURCE="$(sha256sum "${TMP_DIR}/depsguard-source.tar.gz" | awk '{print $1}')"
export VERSION GITHUB_REPOSITORY SHA256_SOURCE

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
envsubst <"${ROOT}/packaging/homebrew/depsguard.rb.in" >"${TMP_DIR}/depsguard.rb"

git clone --depth 1 "https://x-access-token:${HOMEBREW_CORE_TOKEN}@github.com/${CORE_FORK}.git" "${TMP_DIR}/homebrew-core"
cd "${TMP_DIR}/homebrew-core"
git checkout -B "${BRANCH}"
mkdir -p Formula/d
cp "${TMP_DIR}/depsguard.rb" Formula/d/depsguard.rb

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add Formula/d/depsguard.rb

if git diff --cached --quiet; then
  echo "Homebrew formula unchanged; nothing to commit."
else
  git commit -m "depsguard ${VERSION}"
  git push -u origin "${BRANCH}" --force-with-lease
fi

export GH_TOKEN="${HOMEBREW_CORE_TOKEN}"
PR_TITLE="depsguard ${VERSION}"
PR_BODY="$(cat <<EOF
## Summary
- update \`depsguard\` formula to ${VERSION}
- set source tarball sha256 for \`${TAG}\`
- keep Rust source build and formula test

## Test plan
- [x] \`brew style --fix Formula/d/depsguard.rb\`
- [ ] \`brew install --build-from-source ./Formula/d/depsguard.rb\`
- [ ] \`brew audit --strict --new --online depsguard\`
- [ ] \`brew test depsguard\`
EOF
)"

if gh pr view -R "${BASE_REPO}" --head "${FORK_OWNER}:${BRANCH}" >/dev/null 2>&1; then
  gh pr edit -R "${BASE_REPO}" --head "${FORK_OWNER}:${BRANCH}" --title "${PR_TITLE}" --body "${PR_BODY}"
  echo "Updated existing Homebrew core PR for ${FORK_OWNER}:${BRANCH}."
else
  gh pr create -R "${BASE_REPO}" --head "${FORK_OWNER}:${BRANCH}" --base main --title "${PR_TITLE}" --body "${PR_BODY}"
  echo "Created Homebrew core PR for ${FORK_OWNER}:${BRANCH}."
fi
