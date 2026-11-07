#!/usr/bin/env python3
"""Version checks and artifact manifests for the version-build workflow (stdlib only)."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "dist" / "release-assets"


def versions():
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    version = re.search(r'^version = "([^"]+)"$', cargo, re.M)[1]
    if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", version):
        raise ValueError("Desktop releases require a numeric major.minor.patch version")
    if any(int(part) > 65535 for part in version.split(".")):
        raise ValueError("Desktop version exceeds Windows installer version limits")
    lock = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
    if not re.search(r'name = "doubao-microphone"\nversion = "' + re.escape(version) + '"', lock):
        raise ValueError("Cargo.lock package version differs from Cargo.toml")
    for filename in ("web/package.json", "web/package-lock.json"):
        package = json.loads((ROOT / filename).read_text(encoding="utf-8"))
        if package["version"] != version:
            raise ValueError(f"{filename} version differs from Cargo.toml")
        if "packages" in package and package["packages"][""]["version"] != version:
            raise ValueError("web/package-lock.json root package version differs")
    for filename in ("Info.plist", "AgentInfo.plist", "MacInstallerInfo.plist"):
        with (ROOT / "packaging" / filename).open("rb") as stream:
            info = plistlib.load(stream)
        for key in ("CFBundleVersion", "CFBundleShortVersionString"):
            if key in info and info[key] != version:
                raise ValueError(f"packaging/{filename}: {key} differs from Cargo.toml")
    installer = (ROOT / "packaging/windows.iss").read_text(encoding="utf-8")
    if f'#define AppVersion "{version}"' not in installer:
        raise ValueError("Windows installer fallback version differs from Cargo.toml")
    project = (ROOT / "ios/DoubaoMicrophone.xcodeproj/project.pbxproj").read_text(encoding="utf-8")
    ios_versions = set(re.findall(r"MARKETING_VERSION = ([\d.]+);", project))
    ios_builds = set(re.findall(r"CURRENT_PROJECT_VERSION = (\d+);", project))
    if len(ios_versions) != 1 or len(ios_builds) != 1:
        raise ValueError("iOS Debug and Release version/build must agree")
    android = (ROOT / "android/app/build.gradle").read_text(encoding="utf-8")
    android_version = re.search(r"versionName '([\d.]+)'", android)[1]
    android_build = re.search(r"versionCode (\d+)", android)[1]
    return {"version": version, "tag": f"v{version}",
            "ios_version": ios_versions.pop(), "ios_build": ios_builds.pop(),
            "android_version": android_version, "android_build": android_build}


def check_version(requested):
    values = versions()
    requested = requested or values["version"]
    if requested not in (values["version"], values["tag"]):
        raise ValueError("Requested version/tag differs from source; update version files first")
    if os.environ.get("GITHUB_REF_TYPE") == "tag":
        if os.environ["GITHUB_REF_NAME"] != values["tag"]:
            raise ValueError("Tag must exactly match v + Cargo.toml version")
    values["commit"] = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    # A manual build may reuse a tag only when it points to precisely this commit.
    tag_ref = f"refs/tags/{values['tag']}"
    tag = subprocess.run(["git", "rev-parse", "--verify", f"{tag_ref}^{{commit}}"],
                         cwd=ROOT, text=True, capture_output=True)
    if tag.returncode == 0 and tag.stdout.strip() != values["commit"]:
        raise ValueError("This version tag already points to another commit; bump the version")
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
            for key, value in values.items():
                stream.write(f"{key}={value}\n")
    print(json.dumps(values, ensure_ascii=False, indent=2))


def package(kind):
    values = versions()
    ASSETS.mkdir(parents=True, exist_ok=True)
    version = values["version"]
    if kind == "windows-x64":
        installer = ROOT / "dist" / f"DoubaoMicrophone-Setup-{version}-x64-umdf-trial.exe"
        shutil.copy2(installer, ASSETS / installer.name)
    elif kind.startswith("macos-"):
        arch = kind.removeprefix("macos-")
        dmg = ROOT / "dist" / f"DoubaoMicrophone-{version}-macos-{arch}-test.dmg"
        shutil.copy2(dmg, ASSETS / dmg.name)
    elif kind == "ios":
        prefix = f"DoubaoMicrophone-iOS-{values['ios_version']}-build{values['ios_build']}"
        app = ROOT / "ios/build/device/Build/Products/Release-iphoneos/DoubaoMicrophone.app"
        subprocess.run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent",
                        str(app), str(ASSETS / f"{prefix}-device-unsigned.zip")], check=True)
    elif kind == "android":
        apk = ROOT / "android/app/build/outputs/apk/debug/app-debug-unsigned.apk"
        shutil.copy2(apk, ASSETS / f"DoubaoMicrophone-Android-{values['android_version']}-build{values['android_build']}-unsigned.apk")
    else:
        raise ValueError(f"Unsupported package kind: {kind}")
    values.update(platform=kind, commit=os.environ.get("GITHUB_SHA", "local"),
                  run_url=(f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/"
                           f"{os.environ.get('GITHUB_REPOSITORY', '')}/actions/runs/"
                           f"{os.environ.get('GITHUB_RUN_ID', '')}"),
                  signing={"windows-x64": "unsigned application/installer; UMDF driver uses the 100-year trial certificate; VB-CABLE retains vendor signature",
                           "ios": "unsigned; device product requires your own development signing",
                           "android": "unsigned CI debug APK; sign locally with the existing key before distribution"}.get(
                               kind, "ad-hoc development signature; not notarized"))
    (ASSETS / f"build-{kind}.json").write_text(
        json.dumps(values, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    checksums(ASSETS, f"SHA256SUMS-{kind}.txt")


def checksums(folder, name):
    paths = sorted(p for p in folder.iterdir() if p.is_file() and not p.name.startswith("SHA256SUMS"))
    if not paths:
        raise ValueError(f"No release assets in {folder}")
    lines = []
    for path in paths:
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
        lines.append(f"{digest.hexdigest()}  {path.name}\n")
    (folder / name).write_text("".join(lines), encoding="utf-8")


def public_asset_names(values=None):
    """The four user-facing release attachments, across every version."""
    values = values or versions()
    version = values["version"]
    return [
        f"DoubaoMicrophone-{version}-macos-aarch64-test.dmg",
        f"DoubaoMicrophone-Android-{values['android_version']}-build{values['android_build']}-debug.apk",
        f"DoubaoMicrophone-iOS-{values['ios_version']}-build{values['ios_build']}-device-unsigned.zip",
        f"DoubaoMicrophone-Setup-{version}-x64-umdf-trial.exe",
    ]


def public_assets(folder, values=None, include_android=True):
    names = public_asset_names(values)
    if not include_android:
        names = [name for name in names if not name.endswith("-debug.apk")]
    paths = [folder / name for name in names]
    missing = [path.name for path in paths if not path.is_file()]
    if missing:
        raise ValueError("Missing public release files: " + ", ".join(missing))
    return paths


def stage_public_assets(folder, output):
    """Stage exactly four verified binaries; records remain in Actions/local cache."""
    selected = public_assets(folder)
    entries = dict(line.split("  ", 1)[::-1]
                   for line in (folder / "SHA256SUMS.txt").read_text().splitlines())
    for path in selected:
        if entries.get(path.name) != hashlib.sha256(path.read_bytes()).hexdigest():
            raise ValueError(f"Artifact differs from its checksum: {path.name}")
    if output.resolve() == folder.resolve():
        raise ValueError("Public output must be separate from internal build records")
    if output.exists() and any(output.iterdir()):
        raise ValueError("Use an empty public output directory")
    output.mkdir(parents=True, exist_ok=True)
    for path in selected:
        shutil.copy2(path, output / path.name)
    print(json.dumps({"public_files": [path.name for path in selected], "folder": str(output)},
                     ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    version = sub.add_parser("version")
    version.add_argument("--requested-version", default="")
    bundle = sub.add_parser("package")
    bundle.add_argument("kind", choices=["windows-x64", "macos-aarch64", "macos-x86_64", "ios", "android"])
    sums = sub.add_parser("checksums")
    sums.add_argument("folder", type=Path)
    public = sub.add_parser("public-assets")
    public.add_argument("folder", type=Path, help="Candidate with locally signed Android APK and UMDF trial installer")
    public.add_argument("--output", type=Path, required=True, help="Empty directory for the four attachments")
    args = parser.parse_args()
    if args.command == "version":
        check_version(args.requested_version)
    elif args.command == "package":
        package(args.kind)
    elif args.command == "public-assets":
        stage_public_assets(args.folder, args.output)
    else:
        checksums(args.folder, "SHA256SUMS.txt")


if __name__ == "__main__":
    main()
