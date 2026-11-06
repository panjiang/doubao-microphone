#!/usr/bin/env python3
"""Publish build assets to a draft; never edit an already published release."""

import json
import os
from pathlib import Path
import subprocess

def main():
    root = Path(__file__).resolve().parent.parent
    tag, commit = os.environ["TAG"], os.environ["COMMIT"]
    repo = os.environ["GITHUB_REPOSITORY"]
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
        f"## 豆包远程 {tag} · 开发预览\n\n"
        f"源码提交：[`{commit[:12]}`](https://github.com/{repo}/commit/{commit})\n\n"
        f"- Windows x64：安装包包含官方 VB-CABLE；便携包需要自行准备。"
        "应用和安装器未做代码签名，**不含实验性 UMDF 键盘组件**；"
        "备用工具栏模式受豆包版本限制。\n"
        "- macOS：Apple Silicon / Intel 独立测试 DMG，ad-hoc 签名，**未公证**。"
        "构建成功不代表 Intel 或全部语音链路已经实机验证。\n"
        f"- iOS {os.environ['IOS_VERSION']}（构建 {os.environ['IOS_BUILD']}）："
        "模拟器包与**未签名**真机产物；真机使用需自行开发签名，不能直接安装。\n"
        f"- Android {os.environ['ANDROID_VERSION']}（构建 {os.environ['ANDROID_BUILD']}）："
        "Android 10+ 原生客户端。CI 产物**未签名**，须维护者在本机使用稳定密钥签名，替换 APK 与校验清单后再公开；安卓真机链路尚未验收。\n\n"
        f"[安装与平台限制](https://github.com/{repo}/blob/{commit}/docs/getting-started.md) · "
        f"[验证范围](https://github.com/{repo}/blob/{commit}/docs/verification.md)\n\n"
        "附各平台构建清单与 SHA256SUMS.txt。CI 只构建打包，未运行录音、按键或实机验收。\n",
        encoding="utf-8")
    if not matches:
        # --target also creates a missing tag at precisely the compiled source commit.
        subprocess.run(["gh", "release", "create", tag, "--repo", repo,
                        "--target", commit, "--title", f"豆包远程 {tag} · 开发预览",
                        "--notes-file", str(notes), "--draft", "--prerelease"], check=True)
    else:
        if matches[0]["target_commitish"] != commit:
            raise RuntimeError("Existing draft targets another commit; refusing to replace assets")
        subprocess.run(["gh", "release", "edit", tag, "--repo", repo,
                        "--notes-file", str(notes)], check=True)
    assets = sorted(str(p) for p in (root / "dist/release-assets").iterdir() if p.is_file())
    if not assets:
        raise RuntimeError("No build assets to upload")
    subprocess.run(["gh", "release", "upload", tag, "--repo", repo, "--clobber", *assets], check=True)
    url = subprocess.check_output(["gh", "release", "view", tag, "--repo", repo,
                                   "--json", "url", "--jq", ".url"], text=True).strip()
    print(f"Draft release: {url}")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write(f"[打开草稿 Release]({url})（尚未公开）\n")


if __name__ == "__main__":
    main()
