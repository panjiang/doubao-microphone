# 豆包麦克风首版标识

首版 0.1.0 按全新安装交付。首次启动使用独立设置、配对和 CA，手机重新扫码。旧应用及其本机数据由使用者自行处理。

| 位置 | 标识 |
| --- | --- |
| GitHub、Rust 包及桌面命令 | GitHub 仓库为 panjiang/doubao-microphone，Rust 包和命令为 doubao-microphone |
| Windows | AppId 为 88f8d6f7-3dca-4018-9bf0-d6f4ea807bea，安装目录和当前用户 Run 项使用 DoubaoMicrophone |
| macOS | 主应用、输入助手和安装器使用 local.doubao-microphone.app、.input、.installer，LaunchAgent 和 CA 钥匙串服务使用 local.doubao-microphone |
| iOS、Android | iOS Bundle ID 为 local.doubao-microphone.ios，Android application ID 为 local.doubao.microphone，两端构建号均为 1 |
| 配对和驱动 | 网页凭据键使用 doubao-microphone 前缀，内部 UMDF 试用包使用 ROOT\DOUBAOMICROPHONE_UMDF_TRIAL |

音频帧和配对协议延续现有格式。旧名称的历史构建与实测见[验证记录](verification.md)。
