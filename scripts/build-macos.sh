#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export MACOSX_DEPLOYMENT_TARGET=14.2
cargo_flags=(--locked)
if [[ "${OFFLINE:-0}" == 1 ]]; then cargo_flags+=(--offline); fi
if [[ "${SKIP_FRONTEND:-0}" != 1 ]]; then
  (cd web; npm_flags=(ci --ignore-scripts); if [[ "${OFFLINE:-0}" == 1 ]]; then npm_flags+=(--offline); fi; npm "${npm_flags[@]}"; npm run build)
fi
if [[ "${SKIP_TESTS:-0}" != 1 ]]; then cargo test "${cargo_flags[@]}" --all-targets; fi
[[ -f assets/icons/DoubaoMicrophone.icns ]] || swift scripts/generate-macos-icons.swift
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
sign_app() {
  if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    if [[ "${APPLE_SIGNING_TIMESTAMP:-auto}" == none ]]; then
      codesign --force --options runtime --timestamp=none --sign "$APPLE_SIGNING_IDENTITY" "$1"
    else
      codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$1"
    fi
  else
    codesign --force --sign - "$1"
  fi
  codesign --verify --deep --strict "$1"
}
for arch in ${MACOS_ARCHS:-aarch64 x86_64}; do
  case "$arch" in aarch64|x86_64) ;; *) printf 'Unsupported architecture: %s\n' "$arch" >&2; exit 1;; esac
  target="${arch}-apple-darwin"
  cargo build --release "${cargo_flags[@]}" --target "$target"
  app="dist/${arch}/豆包麦克风.app"
  rm -rf "$app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "target/$target/release/doubao-microphone" "$app/Contents/MacOS/doubao-microphone"
  cp packaging/Info.plist "$app/Contents/Info.plist"
  cp assets/icons/DoubaoMicrophone.icns "$app/Contents/Resources/"
  mkdir -p "$app/Contents/Resources/zh-Hans.lproj"
  cp packaging/zh-Hans.lproj/InfoPlist.strings "$app/Contents/Resources/zh-Hans.lproj/"
  # Sign the nested input helper before signing the containing application.
  helper="$app/Contents/Resources/DoubaoMicrophoneInput.app"
  mkdir -p "$helper/Contents/MacOS"
  mkdir -p "$helper/Contents/Resources"
  cp "target/$target/release/doubao-microphone" "$helper/Contents/MacOS/doubao-microphone"
  cp packaging/AgentInfo.plist "$helper/Contents/Info.plist"
  cp assets/icons/DoubaoMicrophone.icns "$helper/Contents/Resources/"
  sign_app "$helper"
  sign_app "$app"
  if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    printf '%s\n' 'Development signature only: NOT a notarized distribution.' >&2
  fi
  stage="dist/${arch}/pkg"
  rm -rf "$stage"
  root="$stage/root"
  mkdir -p "$root/Applications"
  ditto "$app" "$root/Applications/豆包麦克风.app"
  codesign --verify --deep --strict "$root/Applications/豆包麦克风.app"
  components="$stage/components.plist"
  pkgbuild --analyze --root "$root" "$components"
  python3 - "$components" <<'PY'
import plistlib
import sys

with open(sys.argv[1], "rb") as stream:
    components = plistlib.load(stream)
for component in components:
    if component["RootRelativeBundlePath"] == "Applications/豆包麦克风.app":
        component["BundleIsRelocatable"] = False
        break
else:
    raise SystemExit("Main application was not found in pkgbuild's component list")
with open(sys.argv[1], "wb") as stream:
    plistlib.dump(components, stream)
PY
  component="$stage/DoubaoMicrophone-component.pkg"
  pkgbuild --root "$root" --install-location / --identifier local.doubao-microphone.pkg \
    --version "$version" --ownership recommended --component-plist "$components" \
    --scripts packaging/macos-pkg-scripts "$component"
  inspected="$stage/inspected-component"
  pkgutil --expand "$component" "$inspected"
  python3 - "$inspected/PackageInfo" <<'PY'
import sys
import xml.etree.ElementTree as ET

info = ET.parse(sys.argv[1]).getroot()
if info.get("install-location") != "/":
    raise SystemExit("macOS package has an unexpected install location")
if info.find("relocate/bundle") is not None:
    raise SystemExit("macOS package could relocate the application outside /Applications")
if info.find("bundle[@path='./Applications/豆包麦克风.app']") is None:
    raise SystemExit("macOS package does not install the main app to /Applications")
PY
  distribution="$stage/Distribution.xml"
  productbuild --synthesize --package "$component" "$distribution"
  python3 - "$distribution" "${arch/aarch64/arm64}" <<'PY'
import sys
import xml.etree.ElementTree as ET

tree = ET.parse(sys.argv[1])
root = tree.getroot()
root.insert(0, ET.Element("title"))
root.find("title").text = "豆包麦克风"
script = ET.Element("script")
script.text = "var doubaoNeedsRestart = !system.files.fileExistsAtPath('/Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver');"
root.insert(1, script)
options = root.find("options")
options.set("hostArchitectures", sys.argv[2])
options.set("require-scripts", "true")
root.findall("pkg-ref")[-1].set("onConclusionScript", "doubaoNeedsRestart ? 'RequireRestart' : 'None'")
tree.write(sys.argv[1], encoding="utf-8", xml_declaration=True)
PY
  package="dist/DoubaoMicrophone-${version}-macos-${arch/aarch64/arm64}.pkg"
  product_args=(--distribution "$distribution" --package-path "$stage")
  if [[ -n "${APPLE_INSTALLER_SIGNING_IDENTITY:-}" ]]; then
    product_args+=(--sign "$APPLE_INSTALLER_SIGNING_IDENTITY")
  fi
  productbuild "${product_args[@]}" "$package"
  if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    [[ "${APPLE_SIGNING_IDENTITY:-}" == "Developer ID Application:"* ]] || { printf '%s\n' 'Notarization requires Developer ID signing' >&2; exit 1; }
    [[ "${APPLE_INSTALLER_SIGNING_IDENTITY:-}" == "Developer ID Installer:"* ]] || { printf '%s\n' 'Notarization requires a Developer ID Installer identity' >&2; exit 1; }
    xcrun notarytool submit "$package" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$package"
  fi
  shasum -a 256 "$package"
done
