#!/usr/bin/env python3
"""Sign downloaded CI Android assets locally; private keys never leave this machine."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets-dir", type=Path, required=True)
    parser.add_argument("--apksigner", type=Path, required=True, help="Android SDK lib/apksigner.jar")
    parser.add_argument("--keystore", type=Path, required=True)
    parser.add_argument("--alias", required=True)
    parser.add_argument("--password-env", required=True, help="Environment variable holding the signing password")
    parser.add_argument("--expected-certificate", required=True, help="SHA-256 of the expected existing signing certificate")
    parser.add_argument("--public-dir", type=Path, help="Empty output folder for the four public attachments")
    args = parser.parse_args()
    assets = args.assets_dir
    manifest = assets / "build-android.json"
    data = json.loads(manifest.read_text(encoding="utf-8"))
    if not re.fullmatch(r"\d+\.\d+\.\d+", str(data["android_version"])) or not str(data["android_build"]).isdigit():
        raise RuntimeError("Invalid Android version in build manifest")
    prefix = f"DoubaoRemote-Android-{data['android_version']}-build{data['android_build']}"
    unsigned = assets / f"{prefix}-unsigned.apk"
    signed = assets / f"{prefix}-debug.apk"
    checksums = dict((line.split("  ", 1)[1], line.split("  ", 1)[0])
                     for line in (assets / "SHA256SUMS.txt").read_text().splitlines())
    if checksums.get(unsigned.name) != digest(unsigned):
        raise RuntimeError("Unsigned APK differs from the CI checksum")
    java = ["java", "-jar", str(args.apksigner)]
    subprocess.run([*java, "sign", "--ks", str(args.keystore), "--ks-key-alias", args.alias,
                    "--ks-pass", f"env:{args.password_env}", "--key-pass", f"env:{args.password_env}",
                    "--out", str(signed), str(unsigned)], check=True)
    verification = subprocess.check_output([*java, "verify", "--verbose", "--print-certs", str(signed)], text=True)
    certificate = re.search(r"Signer #1 certificate SHA-256 digest: ([0-9a-f]+)", verification)[1]
    if certificate != args.expected_certificate.lower().replace(":", ""):
        raise RuntimeError("Signer changed; refusing to replace the release APK")
    data.update(signing="local development/test signature; debug APK, not a Play Store release",
                artifact=signed.name, artifact_sha256=digest(signed),
                unsigned_artifact_sha256=digest(unsigned), signer_certificate_sha256=certificate)
    manifest.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (assets / "SHA256SUMS-android.txt").write_text(
        "".join(f"{digest(path)}  {path.name}\n" for path in sorted([signed, manifest])), encoding="utf-8")
    unsigned.unlink()
    # apksigner may create an auxiliary v4 signature, not needed for normal APK installs.
    signed.with_suffix(".apk.idsig").unlink(missing_ok=True)
    subprocess.run([sys.executable, str(Path(__file__).with_name("ci-release.py")), "checksums", str(assets)], check=True)
    if args.public_dir:
        subprocess.run([sys.executable, str(Path(__file__).with_name("ci-release.py")),
                        "public-assets", str(assets), "--output", str(args.public_dir)], check=True)
    print(f"Signed {signed.name}; existing certificate verified: {certificate}")


if __name__ == "__main__":
    main()
