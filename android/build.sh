#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./gradlew --no-daemon --console plain "$@" :app:assembleDebug
mkdir -p ../dist/android
version=$(sed -n "s/.*versionName '\([^']*\)'.*/\1/p" app/build.gradle)
[[ -n "$version" ]] || { printf '%s\n' 'Cannot read Android versionName' >&2; exit 1; }
build=$(sed -n "s/.*versionCode \([0-9]*\).*/\1/p" app/build.gradle)
[[ -n "$build" ]] || { printf '%s\n' 'Cannot read Android versionCode' >&2; exit 1; }
cp app/build/outputs/apk/debug/app-debug.apk "../dist/android/DoubaoRemote-Android-${version}-build${build}-debug.apk"
