<img src="assets/icons/doubao-ptt.svg" alt="豆包语音遥控" width="72" height="72">

# 豆包语音遥控

用 iPhone 或 Android 手机遥控电脑端豆包输入法：按住说话，松手完成输入。

手机同时充当无线麦克风和遥控器，电脑上的豆包输入法负责识别，将文字写入当前输入框。桌面端面向 Windows 和 macOS，手机可使用 iPhone / Android 原生 App「豆包遥控」或 iPhone Safari 网页。

**电脑光标放入输入框 → 手机按住说话 → 豆包识别输入 → 松手结束**

## 功能

- **一个二维码配对**：App 和手机相机扫描同一个码，无需选择连接方式。
- **松手回车**：正常结束后默认等待 1 秒按 Enter，也可通过滑出操作区域仅完成听写。
- **按需连接**：按住时自动准备麦克风和连接；中断后再次按住可重连。
- **桌面托盘运行**：通过本机网页配置设备、快捷键、回车等待和登录自启动。
- **环境检查**：检测虚拟麦克风、豆包运行状态及可确认的相关设置。

## 支持状态

当前为**开发预览版**。[下载 v0.2.4 安装包及构建产物](https://github.com/panjiang/doubao-voice-remote/releases/tag/v0.2.4)，也可从源码构建。使用前请先阅读平台限制。

| 平台 | 要求 | 当前状态 |
| --- | --- | --- |
| Windows | Windows 11 x64 | 已验证手机到豆包的输入链路；UMDF 键盘组件仍为研究实现，普通安装包不包含它。备用兼容模式受豆包版本限制 |
| macOS / Apple Silicon | macOS 14.2+ | 已构建并验证 Fn 触发、虚拟音频及延迟 Enter；现有包为开发签名，未完成正式公证 |
| macOS / Intel | macOS 14.2+ | 已通过 CI 构建，尚未完成实机验证 |
| iPhone App | iOS 18+ | 原生 SwiftUI App，当前需自行开发签名安装，未提供 App Store / TestFlight 分发 |
| Android App | Android 10+ | 原生 Kotlin App，提供测试签名 APK，真机语音链路待验证 |
| 手机网页 | iOS 18+ Safari | 可使用同一个二维码进入；首次需手动安装并信任电脑证书 |

Linux 仅提供开发模拟模式。已验证的版本、场景和剩余项目见[验证记录](docs/verification.md)，不能据此保证所有豆包版本和电脑环境均可用。

## 开始使用

先按照[安装与首次配置](docs/getting-started.md)准备桌面程序、豆包输入法和虚拟音频设备。

1. **连接同一局域网**：手机使用 Wi-Fi，电脑可接网线；避免隔离的访客网络。
2. **检查电脑环境**：从托盘或菜单栏打开配置页，确认豆包已运行、音频路由正确，并完成必要的系统授权。
3. **扫码配对**：点击「生成配对二维码」，用 App 或手机相机扫描。二维码五分钟内单次有效，成功后自动收起。
4. **按住说话**：把电脑光标放入目标输入框，在手机上按住并说话。

| 手机入口 | 按住区域 | 松手后按 Enter | 仅完成听写 |
| --- | --- | --- | --- |
| 豆包遥控 App | 顶栏以下的区域 | 在按下位置出现的固定圆内松手 | 滑出圆外后松手；滑回圆内可恢复回车 |
| Safari 网页 | 「按住说话」按钮 | 在按钮内正常松手 | 向上滑出按钮后松手；滑回按钮可恢复回车 |

Enter 由当前电脑应用决定是发送消息还是换行。等待时间可在配置页调整；固定等待不代表已经确认豆包写完文字。等待期间再次按住会取消上一次待发的 Enter。

手机需保持前台使用。锁屏、切后台、来电或断线会结束本次输入，恢复后重新按住；不支持后台或锁屏录音。

## 音频怎么连接

| 电脑 | 本程序输出到 | 豆包输入法选择的麦克风 |
| --- | --- | --- |
| Windows | CABLE Input | CABLE Output |
| macOS | BlackHole 2ch | BlackHole 2ch |

Windows 完整安装器会准备 VB-CABLE；Mac 安装向导在缺少设备时从官方来源获取 BlackHole。它们是虚拟音频设备，负责把手机声音送到豆包，不能用普通扬声器代替。更多步骤见[安装说明](docs/getting-started.md)。

## 数据与权限

- 手机与电脑通过局域网连接，音频使用 HTTPS/WSS 传输。本项目不提供云端语音转发；语音识别由豆包输入法完成。
- 默认不保存录音或识别正文。「故障录音诊断」需手动开启，采集文件仅保存在电脑本地，可自行删除。
- App 使用相机扫码、麦克风采音，并访问局域网；iPhone 配对凭据保存在 Keychain，Android 使用 Keystore 密钥加密保存在应用私有目录。
- App 在内部信任扫码指定的电脑证书，无需安装系统根证书；Safari 网页首次使用需要手动完成证书信任。
- 可在电脑配置页撤销配对。卸载本程序不会自动卸载共享的虚拟音频设备。

## 开发与文档

桌面端使用 Rust、Tokio、Axum 和 CPAL；网页使用 TypeScript / Vite；iPhone App 使用 SwiftUI、UIKit 和 AVAudioEngine；Android 使用 Kotlin、原生 View 和 AudioRecord。各端均在本仓库维护。

| 目录 | 内容 |
| --- | --- |
| `src/` | 桌面程序、共享会话逻辑及系统适配 |
| `web/` | 电脑配置页和手机网页 |
| `ios/` | 原生 iPhone App 与 Xcode 工程 |
| `android/` | 原生 Android App 与 Gradle 工程 |
| `packaging/`、`scripts/` | 安装器、资源生成与构建脚本 |
| `docs/` | 使用说明、协议与验证记录 |
| `research/` | Windows 输入兼容性与驱动研究 |

- [安装与首次配置](docs/getting-started.md)
- [开发与构建](docs/development.md)
- [Actions 版本构建与草稿 Release](docs/releases.md)
- [iPhone App 构建与安装](ios/README.md)
- [Android APK 构建与安装](android/README.md)
- [故障排查与卸载](docs/troubleshooting.md)
- [双平台架构](docs/cross-platform.md) · [通信协议](docs/protocol.md) · [统一配对二维码](docs/native-ios-protocol.md)

反馈问题可提交 [Issue](https://github.com/panjiang/doubao-voice-remote/issues)，请说明系统、豆包版本、手机入口和具体表现。

## 许可

本项目源码采用 [MIT License](LICENSE)。豆包输入法、VB-CABLE 和 BlackHole 由各自提供方维护，遵循各自的使用及分发条款。
