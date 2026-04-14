#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script only supports macOS."
  exit 1
fi

PRODUCT_NAME="$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json', 'utf8')).productName")"
VERSION="$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json', 'utf8')).version")"
ARCH="$(uname -m)"

APP_PATH="src-tauri/target/release/bundle/macos/${PRODUCT_NAME}.app"
DMG_DIR="src-tauri/target/release/bundle/dmg"
SIGNED_DMG_PATH="${DMG_DIR}/${PRODUCT_NAME}_${VERSION}_${ARCH}-signed.dmg"
SIGNED_ZIP_PATH="${DMG_DIR}/${PRODUCT_NAME}_${VERSION}_${ARCH}-signed.zip"
VOLUME_PATH="/Volumes/${PRODUCT_NAME}"

echo "==> Building ${PRODUCT_NAME} (${VERSION})"
npm run tauri -- build --bundles app

if [[ ! -d "$APP_PATH" ]]; then
  echo "App bundle not found: $APP_PATH"
  exit 1
fi

echo "==> Applying ad-hoc signature"
codesign --force --deep --sign - --verbose=2 "$APP_PATH"

echo "==> Verifying signature"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"

mkdir -p "$DMG_DIR"
rm -f "$SIGNED_DMG_PATH" "$SIGNED_ZIP_PATH"

echo "==> Creating signed zip"
ditto -c -k --sequesterRsrc --keepParent "$APP_PATH" "$SIGNED_ZIP_PATH"

echo "==> Creating signed dmg"
hdiutil create -volname "$PRODUCT_NAME" -srcfolder "$APP_PATH" -ov -format UDZO "$SIGNED_DMG_PATH"

if [[ -d "$VOLUME_PATH" ]]; then
  hdiutil detach "$VOLUME_PATH" >/dev/null 2>&1 || true
fi

echo "==> Verifying app inside dmg"
ATTACH_OUTPUT="$(hdiutil attach -nobrowse -readonly "$SIGNED_DMG_PATH")"
echo "$ATTACH_OUTPUT"

cleanup() {
  if [[ -d "$VOLUME_PATH" ]]; then
    hdiutil detach "$VOLUME_PATH" >/dev/null 2>&1 || true
  fi
}

trap cleanup EXIT

codesign --verify --deep --strict --verbose=2 "${VOLUME_PATH}/${PRODUCT_NAME}.app"

echo "==> Gatekeeper assessment (informational)"
if ! spctl -a -vv "$APP_PATH"; then
  echo "Gatekeeper still rejects ad-hoc signed apps without an Apple Developer certificate."
  echo "Use right click -> Open, or System Settings -> Privacy & Security -> Open Anyway on first launch."
fi

echo
echo "Build complete:"
echo "  App : ${ROOT_DIR}/${APP_PATH}"
echo "  ZIP : ${ROOT_DIR}/${SIGNED_ZIP_PATH}"
echo "  DMG : ${ROOT_DIR}/${SIGNED_DMG_PATH}"
