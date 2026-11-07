#!/usr/bin/env python3
"""Publish build assets to a draft; never edit an already published release."""

import json
import os
from pathlib import Path
import runpy
import subprocess

def main():
    root = Path(__file__).resolve().parent.parent
    tag, commit = os.environ["TAG"], os.environ["COMMIT"]
    repo = os.environ["GITHUB_REPOSITORY"]
    release = runpy.run_path(str(root / "scripts/ci-release.py"))
    values = release["versions"]()
    if tag != values["tag"]:
        raise RuntimeError("Release tag differs from the checked-out source version")
    allowed = set(release["public_asset_names"](values))
    # CI has no private signing key. Upload three products; the APK
    # remains exclusively in Actions until the maintainer signs and uploads it.
    assets = release["public_assets"](root / "dist/release-assets", values, include_android=False)
    # List must succeed: network/auth errors must not be confused with a missing release.
    pages = subprocess.check_output([
        "gh", "api", "--paginate", "--slurp", f"repos/{repo}/releases?per_page=100"], text=True)
    matches = [r for page in json.loads(pages) for r in page if r["tag_name"] == tag]
    if len(matches) > 1:
        raise RuntimeError("More than one release exists for this tag; review manually")
    if matches and not matches[0]["draft"]:
        raise RuntimeError("Release is already public; refusing to replace its assets")
    notes = root / "dist" / "release-notes.md"
    notes.write_text(
        f"源码提交：[`{commit[:12]}`](https://github.com/{repo}/commit/{commit})\n\n"
        f"- Windows x64：安装包包含官方 VB-CABLE 和 UMDF 虚拟键盘。"
        "驱动使用 100 年自签名证书，固定右 Alt，一页确认、一次管理员授权。"
        "应用和安装器未做代码签名。\n"
        "- macOS：Apple Silicon 测试 DMG，ad-hoc 签名，**未公证**。\n"
        f"- iOS {os.environ['IOS_VERSION']}（构建 {os.environ['IOS_BUILD']}）："
        "**未签名** iPhone 真机产物；使用需自行开发签名，不能直接安装。\n"
        f"- Android {os.environ['ANDROID_VERSION']}（构建 {os.environ['ANDROID_BUILD']}）："
        "Android 10+ 原生客户端。未签名构建仅保存在 Actions，须维护者使用原密钥在本机签名，上传签名 APK 后再公开；安卓真机链路尚未验收。\n\n"
        f"[安装与平台限制](https://github.com/{repo}/blob/{commit}/docs/getting-started.md) · "
        f"[验证范围](https://github.com/{repo}/blob/{commit}/docs/verification.md)\n\n"
        "公开 Release 附件仅保留四个下载文件。公开前用 public-assets 整理并核对附件，补齐签名 Android APK。JSON 和校验记录保存在 Actions／本地。CI 未运行录音、按键或实机验收。\n",
        encoding="utf-8")
    if not matches:
        # --target also creates a missing tag at precisely the compiled source commit.
        subprocess.run(["gh", "release", "create", tag, "--repo", repo,
                        "--target", commit, "--title", f"豆包麦克风 {tag} · 开发预览",
                        "--notes-file", str(notes), "--draft", "--prerelease"], check=True)
    else:
        if matches[0]["target_commitish"] != commit:
            raise RuntimeError("Existing draft targets another commit; refusing to replace assets")
        subprocess.run(["gh", "release", "edit", tag, "--repo", repo,
                        "--notes-file", str(notes)], check=True)
    if matches:
        # Clean obsolete draft-only JSON, checksums, simulator/Intel and unsigned
        # files. Never touch an already public release or replace a signed APK
        # with its unsigned CI input when rerunning the same draft.
        for asset in matches[0]["assets"]:
            if asset["name"] not in allowed:
                subprocess.run(["gh", "release", "delete-asset", tag, "--repo", repo,
                                asset["name"], "--yes"], check=True)
    subprocess.run(["gh", "release", "upload", tag, "--repo", repo,
                    "--clobber", *(str(path) for path in assets)], check=True)
    url = subprocess.check_output(["gh", "release", "view", tag, "--repo", repo,
                                   "--json", "url", "--jq", ".url"], text=True).strip()
    print(f"Draft release: {url}")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write(f"[打开草稿 Release]({url})（尚未公开）\n")


if __name__ == "__main__":
    main()
