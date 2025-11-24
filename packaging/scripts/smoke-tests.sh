#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")"/../.. && pwd)"
BUILD_ROOT="${1:-$ROOT/target/package-build}"

log() {
  printf '[nova][smoke] %s\n' "$1"
}

ensure_artifact() {
  local target="$1"
  shift
  if ! "$@"; then
    log "Skipping ${target} (missing tool or failed command)"
    return 1
  fi
  return 0
}

mkdir -p "$BUILD_ROOT"

log "Ensuring release binaries exist"
if [ ! -x "$ROOT/target/release/nova" ] || [ ! -x "$ROOT/target/release/nova-gui" ]; then
  (cd "$ROOT" && cargo build --release --locked --bins)
fi

verify_arch() {
  if ! command -v makepkg >/dev/null 2>&1; then
    log "Arch package test skipped (makepkg missing)"
    return
  fi

  mkdir -p "$BUILD_ROOT/arch"
  if ! ls "$BUILD_ROOT/arch"/*.pkg.tar.zst >/dev/null 2>&1; then
    (cd "$ROOT/packaging" && make arch)
  fi

  local pkg
  pkg="$(ls -1t "$BUILD_ROOT/arch"/*.pkg.tar.zst | head -n1)"
  log "Validating Arch package ${pkg}"
  if command -v bsdtar >/dev/null 2>&1; then
    bsdtar -tf "$pkg" | grep -q 'usr/bin/nova'
  elif command -v zstd >/dev/null 2>&1; then
    zstd -dc "$pkg" | tar -tf - | grep -q 'usr/bin/nova'
  else
    log "Cannot inspect Arch package contents (missing bsdtar and zstd)"
  fi
}

verify_fedora() {
  if ! command -v rpmbuild >/dev/null 2>&1; then
    log "Fedora RPM test skipped (rpmbuild missing)"
    return
  fi

  mkdir -p "$BUILD_ROOT/fedora"
  if ! ls "$BUILD_ROOT/fedora/RPMS/x86_64"/*.rpm >/dev/null 2>&1; then
    (cd "$ROOT/packaging" && make fedora)
  fi

  local rpm
  rpm="$(ls -1t "$BUILD_ROOT/fedora/RPMS/x86_64"/*.rpm | head -n1)"
  log "Validating RPM ${rpm}"
  if command -v rpm >/dev/null 2>&1; then
    rpm -qlp "$rpm" | grep -q '/usr/bin/nova'
  else
    log "rpm command unavailable; skipping RPM content verification"
  fi
}

verify_flatpak() {
  if ! command -v flatpak-builder >/dev/null 2>&1; then
    log "Flatpak test skipped (flatpak-builder missing)"
    return
  fi

  mkdir -p "$BUILD_ROOT/flatpak"
  local app_dir="$BUILD_ROOT/flatpak/app"
  if [ ! -d "$app_dir" ]; then
    (cd "$ROOT/packaging" && make flatpak)
  fi

  if command -v flatpak >/dev/null 2>&1; then
    log "Running nova --version inside flatpak builddir"
    flatpak build --command=./usr/bin/nova "$app_dir" -- --version >/dev/null
  else
    log "flatpak CLI missing; cannot run build test"
  fi
}

verify_appimage() {
  if ! command -v appimage-builder >/dev/null 2>&1; then
    log "AppImage test skipped (appimage-builder missing)"
    return
  fi

  mkdir -p "$BUILD_ROOT/appimage"
  if ! ls "$BUILD_ROOT/appimage"/*.AppImage >/dev/null 2>&1; then
    (cd "$ROOT/packaging" && make appimage)
  fi

  local img
  img="$(ls -1t "$BUILD_ROOT/appimage"/*.AppImage | head -n1)"
  chmod +x "$img"
  log "Executing AppImage ${img} --version"
  APPIMAGE_EXTRACT_AND_RUN=1 "$img" --version >/dev/null
}

verify_debian() {
  if ! command -v dpkg-buildpackage >/dev/null 2>&1; then
    log "Debian package test skipped (dpkg-buildpackage missing)"
    return
  fi

  mkdir -p "$BUILD_ROOT/debian"
  if ! ls "$BUILD_ROOT/debian"/*.deb >/dev/null 2>&1; then
    (cd "$ROOT/packaging" && make debian)
  fi

  local deb
  deb="$(ls -1t "$BUILD_ROOT/debian"/*.deb | head -n1)"
  log "Validating Debian package ${deb}"
  if command -v dpkg >/dev/null 2>&1; then
    dpkg -c "$deb" | grep -q './usr/bin/nova'
  else
    log "dpkg command unavailable; skipping Debian content verification"
  fi
}

verify_arch
verify_debian
verify_fedora
verify_flatpak
verify_appimage

log "Smoke tests complete"
