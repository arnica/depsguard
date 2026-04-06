#!/usr/bin/env bash
# Build a signed APT repository under docs/apt/ from .deb artifacts.
#
# Required environment variables:
#   RELEASE_VERSION  – semver string (e.g. "0.1.14")
#   GPG_KEY_ID       – fingerprint of the imported GPG signing key
#   APT_DIR          – absolute path to the docs/apt directory
#   DEB_DIR          – absolute path to the directory containing .deb files
set -euo pipefail

: "${RELEASE_VERSION:?}"
: "${GPG_KEY_ID:?}"
: "${APT_DIR:?}"
: "${DEB_DIR:?}"

POOL_DIR="${APT_DIR}/pool/main/d/depsguard"
DISTS_DIR="${APT_DIR}/dists/stable"

# ── 1. Populate the pool ─────────────────────────────────────────────
# Remove old debs so the repo only carries the current release.
rm -rf "${POOL_DIR}"
mkdir -p "${POOL_DIR}"

deb_count=0
for deb in "${DEB_DIR}"/depsguard_*.deb; do
  [ -f "$deb" ] || continue
  cp "$deb" "${POOL_DIR}/"
  echo "Copied $(basename "$deb") into pool"
  deb_count=$((deb_count + 1))
done

if [ "$deb_count" -eq 0 ]; then
  echo "error: no .deb files found in ${DEB_DIR}" >&2
  exit 1
fi

# ── 2. Generate Packages indices ─────────────────────────────────────
for arch in amd64 arm64; do
  ARCH_DIR="${DISTS_DIR}/main/binary-${arch}"
  mkdir -p "${ARCH_DIR}"

  # dpkg-scanpackages wants paths relative to the repo root
  (cd "${APT_DIR}" && dpkg-scanpackages --arch "$arch" pool/) \
    > "${ARCH_DIR}/Packages"
  gzip -9 -k -f "${ARCH_DIR}/Packages"

  pkg_count=$(grep -c '^Package:' "${ARCH_DIR}/Packages" || true)
  echo "Generated Packages for ${arch}: ${pkg_count} package(s)"
done

# ── 3. Generate Release file ─────────────────────────────────────────
mkdir -p "${DISTS_DIR}"
apt-ftparchive \
  -o "APT::FTPArchive::Release::Origin=DepsGuard" \
  -o "APT::FTPArchive::Release::Label=DepsGuard" \
  -o "APT::FTPArchive::Release::Suite=stable" \
  -o "APT::FTPArchive::Release::Codename=stable" \
  -o "APT::FTPArchive::Release::Architectures=amd64 arm64" \
  -o "APT::FTPArchive::Release::Components=main" \
  release "${DISTS_DIR}" > "${DISTS_DIR}/Release"

echo "Generated Release file"

# ── 4. Sign the Release ──────────────────────────────────────────────
# Detached signature
gpg --batch --yes --pinentry-mode loopback \
  --default-key "${GPG_KEY_ID}" \
  -abs -o "${DISTS_DIR}/Release.gpg" \
  "${DISTS_DIR}/Release"

# Inline signature (InRelease)
gpg --batch --yes --pinentry-mode loopback \
  --default-key "${GPG_KEY_ID}" \
  --clearsign -o "${DISTS_DIR}/InRelease" \
  "${DISTS_DIR}/Release"

echo "Signed Release (Release.gpg + InRelease)"

# ── 5. Export public key ──────────────────────────────────────────────
gpg --batch --yes --armor --export "${GPG_KEY_ID}" \
  > "${APT_DIR}/gpg.key"

echo "Exported public key to gpg.key"
echo "APT repository built successfully for version ${RELEASE_VERSION}"
