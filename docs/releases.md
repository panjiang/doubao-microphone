# Actions 版本构建

[Version build 工作流](https://github.com/panjiang/doubao-voice-remote/actions/workflows/version-build.yml)负责版本校验、编译、打包及创建草稿 Release。不执行录音、按键或实机测试，也不会公开发布 Release。

## 手动构建当前版本

1. 打开仓库的 **Actions → Version build → Run workflow**。
2. 选择要构建的分支或标签。`version` 留空时读取该提交的 `Cargo.toml`；填写时必须与源码版本一致，可用 `0.2.3` 或 `v0.2.3`。
3. 默认勾选 `draft_release`，全部平台构建成功后创建草稿；取消勾选则只保存 Actions 产物。
4. 在运行页面的 **Artifacts** 下载对应平台产物，保留 30 天。草稿链接显示在 `draft-release` 作业摘要中，需仓库写权限才能查看。

也可通过 GitHub CLI：

```sh
gh workflow run version-build.yml --ref main -f version=0.2.3 -F draft_release=true
```

手动创建草稿时，如版本标签不存在，Release 使用本次构建的准确提交作为目标；GitHub 可在发布草稿时创建标签。已有标签必须指向相同提交，不会移动或覆盖。

## 用版本标签触发

新版本先同步这些文件，再提交：

- `Cargo.toml` 与 `Cargo.lock` 中本项目的版本。
- `web/package.json`、`web/package-lock.json`（包含根 `packages[""]`）的版本。
- `packaging/Info.plist`、`AgentInfo.plist`、`MacInstallerInfo.plist` 的版本。
- `packaging/windows.iss` 中的默认 `AppVersion`。

iOS 保持自己的版本和构建号，在 `ios/DoubaoPTT.xcodeproj/project.pbxproj` 中维护，Debug / Release 必须一致。Android 在 `android/app/build.gradle` 中维护 `versionName` / `versionCode`。手机版本与桌面版本不要求相同。

可先做只读版本核对：

```sh
python3 scripts/ci-release.py version --requested-version 0.2.4
```

确认对应源码已提交后，推送版本标签，例如：

```sh
git tag v0.2.4
git push origin v0.2.4
```

标签必须为 `v` 加源码中的完整数字版本，例如 `v0.2.3`。版本不一致会在编译前失败。标签构建成功后始终创建草稿 Release；不会随普通分支推送自动编译，也不会自行递增版本。

## 产物与当前限制

| 产物 | 说明 |
| --- | --- |
| `DoubaoPTT-Setup-<版本>-x64.exe` | Windows 完整安装器，包含经哈希及签名验证的原版 VB-CABLE；不含研究用 UMDF 键盘组件 |
| `DoubaoPTT-<版本>-windows-x64-portable.zip` | Windows 便携程序，需要另行准备虚拟音频设备 |
| `DoubaoPTT-<版本>-macos-aarch64-test.dmg` | Apple Silicon 开发测试包，ad-hoc 签名，未公证 |
| `DoubaoPTT-<版本>-macos-x86_64-test.dmg` | Intel 开发测试包，ad-hoc 签名，未公证；不代表已实机验证 |
| `DoubaoRemote-iOS-<版本>-build<构建号>-device-unsigned.zip` | 未签名 iPhone `.app`，需自行开发签名，不能直接安装 |
| `DoubaoRemote-iOS-<版本>-build<构建号>-simulator.zip` | Apple Silicon Mac 上的 iOS 模拟器产物 |
| `DoubaoRemote-Android-<版本>-build<构建号>-unsigned.apk` | Android CI 未签名产物，维护者签名后才可安装 |
| `DoubaoRemote-Android-<版本>-build<构建号>-debug.apk` | 维护者使用稳定测试密钥签名后的预览安装包，APK 私钥不上传 CI |
| `build-<平台>.json`、`SHA256SUMS*.txt` | 源码提交、版本、签名状态和文件校验值 |

所有作业使用 GitHub 托管 runner，优先复用 npm、Cargo 和 VB-CABLE 缓存。缺少缓存时先使用仓库现有依赖源；npm 镜像失败时，仅在 CI 工作目录临时回退至官方 npm，保持锁定版本及 integrity 校验，结束后恢复原锁文件。VB-CABLE 只从锁文件指定的官方地址下载并校验；不执行第三方驱动安装、不修改系统级镜像或代理。无需上传本机签名证书、Apple 账号或 SSH 密钥。

## 草稿复核与公开

只有 Windows、macOS 两种架构、iOS 和 Android 全部构建成功，才进入草稿创建步骤。Android 同时执行静态 Lint，不运行测试或安装。失败时可下载已经成功的作业产物，修复后重新运行。

同一版本重跑只允许更新同一提交的草稿及附件；已公开 Release 会被拒绝覆盖。草稿上传中途失败时，保留草稿，重新运行失败作业即可补齐附件。若代码有改动，使用新的版本号，避免把不同源码当作同一版本发布。

草稿正文明确列出签名、驱动和实机验证限制。检查附件、校验值与实际验收结果后，再由维护者在 GitHub 点击 **Publish release**。构建成功不能替代[实机验证](verification.md)。

### Android 本地签名

下载草稿所有附件到一个独立目录，使用自己的稳定密钥、Android SDK 的 `apksigner.jar` 和 JDK 签名；密码通过环境变量提供，脚本不会保存密码或复制私钥：

```sh
python3 scripts/sign-android-release.py \
  --assets-dir dist/release-0.2.3 \
  --apksigner "$ANDROID_HOME/build-tools/35.0.0/lib/apksigner.jar" \
  --keystore /private/path/android-signing.keystore \
  --alias androiddebugkey \
  --password-env ANDROID_SIGNING_PASSWORD \
  --expected-certificate YOUR_EXISTING_CERTIFICATE_SHA256
```

证书指纹应从已知旧 APK 的 `apksigner verify --print-certs` 结果取得。脚本先核对 CI 未签名文件哈希，签名后核对证书指纹，再替换本地 APK、更新 Android 构建清单与校验文件。它不访问 GitHub，也不会公开 Release。上传这些替换文件、删除草稿中的未签名 APK，并将正文改为实际签名状态后，才可发布。
