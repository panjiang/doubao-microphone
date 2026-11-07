# Actions 版本构建

[Version build 工作流](https://github.com/panjiang/doubao-microphone/actions/workflows/version-build.yml)负责版本校验、编译、打包及创建草稿 Release。不执行录音、按键或实机测试，也不会公开发布 Release。

## 手动构建当前版本

1. 打开仓库的 **Actions → Version build → Run workflow**。
2. 选择要构建的分支或标签。`version` 留空时读取该提交的 `Cargo.toml`；填写时必须与源码版本一致，可用 `0.1.0` 或 `v0.1.0`。
3. 默认勾选 `draft_release`，全部平台构建成功后创建草稿；取消勾选则只保存 Actions 产物。
4. 在运行页面的 **Artifacts** 下载对应平台产物，保留 30 天。草稿链接显示在 `draft-release` 作业摘要中，需仓库写权限才能查看。

也可通过 GitHub CLI：

```sh
gh workflow run version-build.yml --ref main -f version=0.1.0 -F draft_release=true
```

手动创建草稿时，如版本标签不存在，Release 使用本次构建的准确提交作为目标；GitHub 可在发布草稿时创建标签。已有标签必须指向相同提交，不会移动或覆盖。

## 用版本标签触发

新版本先同步这些文件，再提交：

- `Cargo.toml` 与 `Cargo.lock` 中本项目的版本。
- `web/package.json`、`web/package-lock.json`（包含根 `packages[""]`）的版本。
- `packaging/Info.plist`、`AgentInfo.plist`、`MacInstallerInfo.plist` 的版本。
- `packaging/windows.iss` 中的默认 `AppVersion`。

iOS 保持自己的版本和构建号，在 `ios/DoubaoMicrophone.xcodeproj/project.pbxproj` 中维护，Debug / Release 必须一致。Android 在 `android/app/build.gradle` 中维护 `versionName` / `versionCode`。手机版本与桌面版本不要求相同。

可先做只读版本核对：

```sh
python3 scripts/ci-release.py version --requested-version 0.1.0
```

确认对应源码已提交后，推送版本标签，例如：

```sh
git tag v0.1.0
git push origin v0.1.0
```

标签必须为 `v` 加源码中的完整数字版本，例如 `v0.1.0`。版本不一致会在编译前失败。标签构建成功后始终创建草稿 Release；不会随普通分支推送自动编译，也不会自行递增版本。

## Release 固定四个附件

每次发版仅编译和提供以下四类下载文件，版本号由源码读取：

| 附件 | 说明 |
| --- | --- |
| `DoubaoMicrophone-<版本>-macos-aarch64-test.dmg` | Apple Silicon 测试包，ad-hoc 签名、未公证 |
| `DoubaoMicrophone-Android-<版本>-build<构建号>-debug.apk` | 本机使用原测试证书签名后的 Android APK |
| `DoubaoMicrophone-iOS-<版本>-build<构建号>-device-unsigned.zip` | 未签名 iPhone 真机产物，需自行开发签名 |
| `DoubaoMicrophone-Setup-<版本>-x64.exe` | Windows 完整安装器，包含原版 VB-CABLE，不含 UMDF 研究键盘驱动 |

Release 不上传 JSON、TXT 校验清单、Intel Mac 包、iOS 模拟器包或未签名 Android APK。Intel 和模拟器仍可按开发说明在本地手动构建，不参加每次版本发布。

构建元数据和校验记录保存在 Actions Artifacts，供维护者检查和本机签名使用。GitHub 自动显示的 Source code 下载链接由平台生成，不属于上传附件。

## 构建与草稿

Windows、Mac Apple Silicon、iPhone 真机和 Android 构建成功后，汇总内部产物并上传为 Actions Artifact：`release-candidate-v<版本>`，保留 30 天。Android 同时执行 Lint；所有作业不运行测试、安装、录音或按键。

CI 不接触本机私钥，所以草稿先上传三个可用文件；Android 未签名 APK 只留在 Actions，签名后再补齐第四个。草稿没有签名 APK 前不要公开。仅使用精确文件名白名单上传，已有草稿重跑时清理非白名单附件；已公开 Release 不由构建工作流覆盖。

所有作业优先复用 npm、Cargo 和 VB-CABLE 缓存，保持锁文件及校验；不修改系统级镜像或代理，不向 CI 上传签名私钥。

## Android 本机签名并准备公开附件

从同一次构建下载完整内部候选包，避免从只有三个附件的 Release 草稿取签名输入：

```sh
# 替换为本次实际运行 ID 和版本。
gh run download RUN_ID --repo panjiang/doubao-microphone \
  --name release-candidate-v0.1.0 --dir dist/candidate-v0.1.0

python3 scripts/sign-android-release.py \
  --assets-dir dist/candidate-v0.1.0 \
  --apksigner "$ANDROID_HOME/build-tools/35.0.0/lib/apksigner.jar" \
  --keystore /private/path/android-signing.keystore \
  --alias androiddebugkey \
  --password-env ANDROID_SIGNING_PASSWORD \
  --expected-certificate YOUR_EXISTING_CERTIFICATE_SHA256 \
  --public-dir dist/public-v0.1.0
```

签名脚本先核对 CI 校验值，再核对原签名证书，更新内部记录，最后向空的 `--public-dir` 复制四个附件。该目录不含 JSON、校验清单或未签名 APK；文件缺失或哈希不匹配会报错。密码由环境变量提供，私钥不上传。

已有签名产物时，也可单独准备公开目录，不重复签名：

```sh
python3 scripts/ci-release.py public-assets dist/candidate-v0.1.0 \
  --output dist/public-v0.1.0
```

检查空目录生成的四个附件后，上传该目录到对应草稿；不要上传内部 candidate 目录。确认 Release 的四个文件名和哈希正确，更新说明中的实际签名状态后再公开。构建和哈希校验不能替代实机验收。
