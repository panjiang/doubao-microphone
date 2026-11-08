# 开发与构建

本仓库同时维护 Windows/macOS 桌面端、电脑配置页、手机网页以及原生 iPhone 和 Android App。界面操作见[安装与首次配置](getting-started.md)，本页面向从源码构建和参与开发的读者。

## 工程结构

- `src/`：桌面端使用 Rust、Tokio、Axum 和 CPAL，负责共享会话、协议、音频队列、配对服务及平台适配。
- `web/`：TypeScript/Vite 页面，构建后嵌入桌面二进制。
- `ios/`：原生 SwiftUI/UIKit App，使用 AVAudioEngine 和系统网络框架。
- `android/`：原生 Kotlin App，使用 Android View 和 AudioRecord。
- `packaging/`、`scripts/`：安装器、原生资源和构建工具。
- `research/`：实验性 Windows 输入组件及验证资料，不等同于正式分发方案。

仓库、Rust 包和桌面命令统一为 `doubao-microphone`，Rust crate 为 `doubao_microphone`，原生工程及应用产物为 `DoubaoMicrophone`。覆盖安装需要保留的旧标识见[统一命名与升级说明](renaming.md)。

## 依赖

| 内容 | 要求 |
| --- | --- |
| Rust | 工程固定为 1.94.0，见 `rust-toolchain.toml` |
| 网页 | Node.js 22+、npm，使用提交的锁文件 |
| Windows | MSVC C++ 构建工具和 Windows SDK，资源编译需要 `rc.exe` |
| macOS | Xcode Command Line Tools；生成安装器需要 Swift 编译器 |
| iOS | 提供 iOS 18+ SDK 的 Xcode，真机安装需要可用签名团队 |

Cargo 的镜像配置位于工程 `.cargo/config.toml`。锁文件随源码提交，构建使用 `--locked`；离线参数要求依赖已缓存。不要为本项目修改系统级代理或全局镜像源。

希望直接在 GitHub 上编译时，使用 [Actions 版本构建](releases.md)：支持手动运行和 `v*` 标签触发，生成各平台版本产物，并可创建草稿 Release。CI 包不携带个人签名证书，不会自动公开发布。

## 本地桌面构建

```sh
git clone https://github.com/panjiang/doubao-microphone.git
cd doubao-microphone
cd web
npm ci --ignore-scripts
npm run build
cd ..
cargo build --locked
```

必须先生成 `web/dist`。修改网页后需要重新构建网页及 Rust 程序，刷新浏览器不会替换已经嵌入二进制的旧资源。

电脑管理界面分为首页 `/` 与独立设置页 `/settings`，沿用同一标签页的管理凭据。首页只展示检查、扫码、连接状态和设备摘要；完整设置、设备管理、高级诊断与帮助在设置页。二维码缓存绑定当前管理会话，往返页面不重新签发未过期二维码。

网页单元测试运行 `npm --prefix web test`。浏览器布局验收运行 `node scripts/admin-smoke.mjs`，默认使用 macOS 的 Google Chrome，可通过 `CHROME_PATH` 指定 Chromium。验收使用隔离浏览器配置和模拟接口，不访问真实麦克风或应用数据，截图写入 `artifacts/admin-ui/`。用 `ADMIN_TEST_PLATFORM=windows node scripts/admin-smoke.mjs` 核对 Windows 的音频设备和触发方式场景，截图写入其 `windows/` 子目录。

在 Windows/macOS 上启动托盘及配置页：

```sh
cargo run --locked -- serve --open
```

Linux 没有原生音频、按键或托盘实现，仅可运行隔离的模拟模式：

```sh
cargo run --locked -- --mock --data-dir ./artifacts/mock-data serve --headless
```

模拟模式不发送真实按键或声音。headless 模式会打印带本机管理令牌的配置 URL，不能将该 URL 或测试密钥用于公开示例、Issue 或正式配对。

## Windows 打包

便携程序：

```powershell
.\scripts\build-windows.ps1 -PortableOnly
```

完整安装器还需要 Inno Setup 6.7+ 和官方原版 `VBCABLE_Driver_Pack45.zip`：

```powershell
.\scripts\build-windows.ps1 -VbCablePackage C:\build-inputs\VBCABLE_Driver_Pack45.zip
```

构建会按 `packaging/vbcable.lock.json` 检查归档哈希及原厂签名。缺少输入时不会自动下载；安装包只集成音频组件，不包含实验性 UMDF 键盘组件。输出位于 `dist/`。

可使用 `-Offline` 复用本机依赖缓存，`-SkipFrontend` 复用已构建网页，`-SkipTests` 跳过脚本中的测试步骤。详细流程及第三方组件说明见 [Windows 安装交付](windows-installer.md)。

## macOS 打包

在对应 Mac 上构建。先安装所需 Rust target，再选择架构：

```sh
# Apple Silicon
MACOS_ARCHS=aarch64 bash scripts/build-macos.sh
# Intel：需要 x86_64-apple-darwin target，尚未完成本项目的实机验证
MACOS_ARCHS=x86_64 bash scripts/build-macos.sh
```

`OFFLINE=1`、`SKIP_FRONTEND=1` 和 `SKIP_TESTS=1` 分别控制离线构建、复用网页和跳过测试。输出包含主应用、输入代理及可直接打开的 PKG。安装时如缺少虚拟麦克风，PKG 会从原发布者下载、校验并安装。

正式发行需要配置 Developer ID Application `APPLE_SIGNING_IDENTITY`、Developer ID Installer `APPLE_INSTALLER_SIGNING_IDENTITY` 和 Keychain 中的 `NOTARY_PROFILE`。Apple Development 或 ad-hoc 签名只用于开发，不能据此声称已完成正式分发。ad-hoc 签名随每次重建变化，重复安装可能使 macOS 再次要求钥匙串和辅助功能授权；长期分发应使用稳定的 Developer ID 签名并完成公证。

当前 macOS 部署基线为 14.2。已构建和实测的架构、权限及兼容性范围见 [macOS 验证记录](macos-validation.md)。

## iPhone App

构建、签名与安装见 [iOS README](../ios/README.md)。App 使用系统框架，没有第三方运行时依赖；构建不需要 XcodeGen 或 CocoaPods。

## Android App

构建、签名与安装见 [Android README](../android/README.md)。

## 检查与验证

网页类型检查包含在 `npm run build` 中。常规代码检查：

```sh
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cd web
npm test
```

统一二维码的 Swift 解析核对可在 Mac 上单独编译运行，需要有效的公钥 CA PEM 文件：

```sh
mkdir -p artifacts
swiftc ios/DoubaoMicrophone/Connection.swift ios/tests/PairingLinkTests.swift -o artifacts/pairing-link-tests
artifacts/pairing-link-tests /path/to/public-ca.pem
```

这组检查验证新链接、旧 JSON 以及无效输入拒绝，不调用麦克风或按键。旧 `scripts/integration.mjs`、`scripts/browser-smoke.mjs` 仍引用已移除的配对审批流程，不能作为当前统一二维码的直接验收入口。

需要核对依赖来源时，可执行 `python3 scripts/verify-dependencies.py`；脚本请求官方元数据，核对锁文件校验值，不执行下载的包。实际豆包和手机行为需按 [P0 实机步骤](p0-validation.md)验证，再将结果写入[验证记录](verification.md)。

## 诊断入口

只读枚举音频设备与平台状态：

```sh
doubao-microphone probe devices
```

`probe key`、`probe audio` 和配置页的快捷键测试会产生真实系统输入，应只在专用测试文档中使用。示例命令的快捷键必须与当前豆包配置一致，API 返回成功不代表豆包已经识别出文字。

- [共享架构与系统差异](cross-platform.md)
- [手机三端交互基准与差异](mobile-parity.md)
- [音频及控制协议](protocol.md)
- [统一配对链接与原生入口](native-ios-protocol.md)
- [Windows 快捷键兼容性研究](windows-input-findings.md)
- [功能与构建验证记录](verification.md)
