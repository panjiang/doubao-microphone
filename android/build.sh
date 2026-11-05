#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./gradlew --no-daemon --console plain "$@" :app:assembleDebug
mkdir -p ../dist/android
cp app/build/outputs/apk/debug/app-debug.apk ../dist/android/DoubaoRemote-0.1.0-android-debug.apk
