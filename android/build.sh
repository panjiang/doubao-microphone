#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./gradlew --no-daemon --console plain "$@" :app:assembleDebug
mkdir -p ../dist/android
version=$(sed -n "s/.*versionName '\([^']*\)'.*/\1/p" app/build.gradle)
[[ -n "$version" ]] || { printf '%s\n' 'Cannot read Android versionName' >&2; exit 1; }
cp app/build/outputs/apk/debug/app-debug.apk "../dist/android/DoubaoRemote-${version}-android-debug.apk"
