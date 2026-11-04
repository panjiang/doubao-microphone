#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
mode="${1:-all}"
bundle="${IOS_BUNDLE_ID:-local.doubao-ptt.ios}"
xcrun swift scripts/GenerateAppIcon.swift
common=(-project DoubaoPTT.xcodeproj -scheme DoubaoPTT -configuration Release "PRODUCT_BUNDLE_IDENTIFIER=$bundle")
if [[ "$mode" == all || "$mode" == simulator ]]; then
  xcodebuild "${common[@]}" -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' -derivedDataPath build/simulator CODE_SIGNING_ALLOWED=NO build
fi
if [[ "$mode" == all || "$mode" == device ]]; then
  xcodebuild "${common[@]}" -sdk iphoneos -destination 'generic/platform=iOS' -derivedDataPath build/device CODE_SIGNING_ALLOWED=NO build
fi
if [[ "$mode" == signed ]]; then
  : "${IOS_DEVELOPMENT_TEAM:?Set IOS_DEVELOPMENT_TEAM to the authorized Apple Development team}"
  if [[ "${IOS_ALLOW_PROVISIONING_UPDATES:-0}" == 1 ]]; then common+=(-allowProvisioningUpdates -allowProvisioningDeviceRegistration); fi
  xcodebuild "${common[@]}" -sdk iphoneos -destination 'generic/platform=iOS' -derivedDataPath build/signed "DEVELOPMENT_TEAM=$IOS_DEVELOPMENT_TEAM" 'CODE_SIGN_IDENTITY=Apple Development' build
fi
case "$mode" in all|simulator|device|signed) ;; *) echo 'Usage: build.sh [all|simulator|device|signed]' >&2;exit 2;; esac
