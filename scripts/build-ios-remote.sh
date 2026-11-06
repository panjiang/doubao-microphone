#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
host="${IOS_BUILD_HOST:-macmini}"
remote='/Users/houzzkitci/codex-builds/doubao-remote-ios'
mode="${1:-all}"
case "$mode" in all|simulator|device) ;; *) printf '%s\n' 'Usage: build-ios-remote.sh [all|simulator|device]; signing/install are explicit remote steps.' >&2;exit 2;; esac
ssh -o BatchMode=yes -o StrictHostKeyChecking=yes "$host" "mkdir -p '$remote'"
rsync -az -e 'ssh -o BatchMode=yes -o StrictHostKeyChecking=yes' --exclude build --exclude xcuserdata "$root/ios/" "$host:$remote/"
ssh -o BatchMode=yes -o StrictHostKeyChecking=yes "$host" "cd '$remote' && bash build.sh '$mode'"
mkdir -p "$root/dist/ios"
rsync -az -e 'ssh -o BatchMode=yes -o StrictHostKeyChecking=yes' "$host:$remote/DoubaoRemote/Assets.xcassets/AppIcon.appiconset/AppIcon.png" "$root/ios/DoubaoRemote/Assets.xcassets/AppIcon.appiconset/"
for target in simulator device; do
  product='Release-iphoneos';[[ "$target" == simulator ]] && product='Release-iphonesimulator'
  if [[ "$mode" == all || "$mode" == "$target" ]]; then
    mkdir -p "$root/dist/ios/$target"
    rsync -az -e 'ssh -o BatchMode=yes -o StrictHostKeyChecking=yes' "$host:$remote/build/$target/Build/Products/$product/DoubaoRemote.app" "$root/dist/ios/$target/"
  fi
done
