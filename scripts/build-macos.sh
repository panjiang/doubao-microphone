#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export MACOSX_DEPLOYMENT_TARGET=14.2
(cd web && npm ci --ignore-scripts && npm run build)
cargo test --locked --all-targets
for arch in aarch64 x86_64; do
  target="${arch}-apple-darwin"
  cargo build --release --locked --target "$target"
  app="dist/${arch}/DoubaoPTT.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "target/$target/release/doubao-ptt" "$app/Contents/MacOS/doubao-ptt"
  cp packaging/Info.plist "$app/Contents/Info.plist"
  # The input agent is a separate, signed app executable with its own TCC identity.
  helper="$app/Contents/Resources/DoubaoPTTInput.app"
  mkdir -p "$helper/Contents/MacOS"
  cp "target/$target/release/doubao-ptt" "$helper/Contents/MacOS/doubao-ptt"
  cp packaging/AgentInfo.plist "$helper/Contents/Info.plist"
  if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$helper"
    codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$app"
    codesign --verify --deep --strict "$app"
  else
    codesign --force --sign - "$helper"
    codesign --force --sign - "$app"
    printf '%s\n' 'Development signature only: NOT a notarized distribution.' >&2
  fi
  dmg="dist/DoubaoPTT-${arch}.dmg"
  hdiutil create -volname DoubaoPTT -srcfolder "$app" -ov -format UDZO "$dmg"
  if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]] || { printf '%s\n' 'Notarization requires Developer ID signing' >&2; exit 1; }
    xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$dmg"
  fi
  shasum -a 256 "$dmg"
done
