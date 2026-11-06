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
[[ -f assets/icons/DoubaoRemote.icns ]] || swift scripts/generate-macos-icons.swift
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
  case "$arch" in aarch64) swift_arch=arm64;; x86_64) swift_arch=x86_64;; *) printf 'Unsupported architecture: %s\n' "$arch" >&2; exit 1;; esac
  target="${arch}-apple-darwin"
  cargo build --release "${cargo_flags[@]}" --target "$target"
  app="dist/${arch}/DoubaoRemote.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "target/$target/release/doubao-remote" "$app/Contents/MacOS/doubao-remote"
  cp packaging/Info.plist "$app/Contents/Info.plist"
  cp assets/icons/DoubaoRemote.icns "$app/Contents/Resources/"
  # The input agent is a separate, signed app executable with its own TCC identity.
  helper="$app/Contents/Resources/DoubaoRemoteInput.app"
  mkdir -p "$helper/Contents/MacOS"
  mkdir -p "$helper/Contents/Resources"
  cp "target/$target/release/doubao-remote" "$helper/Contents/MacOS/doubao-remote"
  cp packaging/AgentInfo.plist "$helper/Contents/Info.plist"
  cp assets/icons/DoubaoRemote.icns "$helper/Contents/Resources/"
  sign_app "$helper"
  sign_app "$app"
  if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    printf '%s\n' 'Development signature only: NOT a notarized distribution.' >&2
  fi
  stage="dist/${arch}/dmg"
  installer="$stage/安装豆包远程.app"
  mkdir -p "$installer/Contents/MacOS" "$installer/Contents/Resources"
  cp packaging/MacInstallerInfo.plist "$installer/Contents/Info.plist"
  cp assets/icons/DoubaoRemote.icns "$installer/Contents/Resources/"
  ditto "$app" "$installer/Contents/Resources/DoubaoRemote.app"
  swiftc -target "${swift_arch}-apple-macos14.2" -O packaging/MacInstaller.swift -o "$installer/Contents/MacOS/DoubaoRemoteInstaller"
  sign_app "$installer"
  printf '%s\n' '双击“安装豆包远程”。缺少 BlackHole 时，安装器从官方来源下载并校验原版安装包，系统会请求管理员授权。已有虚拟麦克风会复用。首次启动默认启用登录自启动，可在配置页关闭。卸载本应用不会自动卸载共享的 BlackHole。辅助功能权限及豆包麦克风选择仍需在系统与豆包设置中完成。' > "$stage/安装说明.txt"
  suffix=-test
  if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    suffix=''
  else
    printf '%s\n' '这是开发测试包，未进行 Developer ID 公证，不作为正式分发版本。具体实测范围见源码 docs/macos-validation.md；仅构建成功不代表所有架构和手机链路已通过。' >> "$stage/安装说明.txt"
  fi
  dmg="dist/DoubaoRemote-${version}-macos-${arch}${suffix}.dmg"
  hdiutil create -volname DoubaoRemote -srcfolder "$stage" -ov -format UDZO "$dmg"
  if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    [[ "${APPLE_SIGNING_IDENTITY:-}" == "Developer ID Application:"* ]] || { printf '%s\n' 'Notarization requires Developer ID signing' >&2; exit 1; }
    xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$dmg"
  fi
  shasum -a 256 "$dmg"
done
