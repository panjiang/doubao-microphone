#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
device="${1:?Usage: install.sh DEVICE_ID [SIGNED_APP_PATH]}"
app="${2:-build/signed/Build/Products/Release-iphoneos/DoubaoPTT.app}"
[[ -d "$app" ]] || { echo 'Signed app missing; run build.sh signed first.' >&2;exit 1; }
codesign --verify --deep --strict "$app"
xcrun devicectl device install app --device "$device" "$app"
