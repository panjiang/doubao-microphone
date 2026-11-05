# Windows 与 macOS 共用逻辑及适配

桌面端在同一个 main 分支、同一个 Rust 工程中维护。版本 0.2.1 补齐 macOS 的界面与业务能力；不同系统只在需要原生能力的地方分开实现。

0.2.2 将两端配置页的手机配对统一为一个二维码，无需选择 App/网页；App 构建 23 解析标准链接并保留旧 JSON 兼容，浏览器沿用原向导。链接格式及信任边界见 [原生 iOS 接入](native-ios-protocol.md)。

| 功能 | 共用部分 | Windows 适配 | macOS 适配 |
| --- | --- | --- | --- |
| 手机连接 | 配对票据、凭据、TLS/WSS、单连接控制、协议版本 1 | DPAPI 保护 CA 私钥 | Keychain 保护 CA 私钥 |
| 听写 | 同一 Session/Controller 状态机、缓冲上限、超时、尾帧与排空 | UMDF 右 Alt 或兼容模式 | CoreGraphics Fn |
| 延迟 Enter | 同一等待、取消、会话编号、重复消息和结果事件逻辑 | SendInput 与前台窗口/控件检查 | CoreGraphics Return 与前台应用/窗口/控件检查 |
| 音频 | CPAL、Rubato、有界队列及采样率转换 | VB-CABLE | BlackHole 2ch |
| 配置页 | 同一 TypeScript 页面、环境列表、二维码、回车等待及高级诊断 | 自动右 Alt，只显示 CABLE Input | 自动 Fn，只显示 BlackHole 2ch |
| 环境检测 | 同一管理鉴权、Report 状态、loading、超时及重试 | Windows 设备/安装记录/JSON 设置 | CoreAudio、进程路径、只读设置归档、辅助功能与当前输入法 |
| 登录启动 | 同一配置开关；新安装默认开启 | 当前用户 Run 项 | 当前用户 LaunchAgent |
| 图标 | 同一原创圆形蓝底白色麦克风 | ICO、托盘 DPI 图像 | ICNS、菜单栏麦克风 |

Mac 的按键和 Enter 均由原有独立输入代理执行，保留租约失效、父进程退出与故障释放流程。Enter 只在正常完成、快捷键释放成功后安排；断开、暂停、新 start、cancel、修改配置及退出均走原有取消逻辑。发送前比较应用 PID、AX 窗口及焦点控件，并检查物理和合并修饰键状态；无法确认时跳过，不切换焦点，不读编辑器正文。固定等待不代表已读取豆包识别完成状态。

Mac 运行环境检查只读取豆包设置存储。读取有大小上限，比较前后元数据并校验 CRC，按最新值解析 MMKV 中的字典归档；仅允许 Foundation 值类型解码，输出仅限语音设置白名单字段。不读取账户、密钥、词库或语音历史文件，不修改豆包配置。跟随默认麦克风、缺字段、存储变化或不认识的编码显示“无法确认”。当前只将已实测的 Fn 编码判为明确匹配。

## macOS 安装

DMG 内包含已签名的原生安装器及其嵌套主应用。双击“安装语音遥控”后，向导安装到 `/Applications/DoubaoPTT.app`，创建当前用户桌面入口，并启动配置页。新安装应用首次启动默认写入当前用户登录启动项；已有 settings.json 的选择保留。

BlackHole 原版已安装时复用。缺失时优先使用当前用户缓存 `~/Library/Caches/DoubaoPTT/BlackHole2ch-0.7.1.pkg`；否则从 `https://existential.audio/downloads/BlackHole2ch-0.7.1.pkg` 下载约 101KB。仅允许 HTTPS，限制下载大小及时间，验证 SHA-256、官方 Developer ID Installer（Existential Audio Inc. / Q5C99V536K）和系统安装评估结果后，请求系统管理员认证并调用官方安装器。提权后的安装命令再次校验哈希与发布者，不修改代理、全局源或安全设置。

锁定 SHA-256：`57b540f27a3e29c37e310e01bee0fdfab76733087e47f997ef9dccf851400dcf`。

不将官方 BlackHole 二进制嵌入第三方发行包；原版安装器及品牌的发行限制见 [BlackHole 官方许可](https://github.com/ExistentialAudio/BlackHole/blob/v0.7.1/LICENSE)。安装时直接获取官方原版，保留来源和系统授权步骤。下载失败或授权取消会结束本次安装，允许重试。辅助功能权限仍由用户在系统设置授权，程序不绕过 TCC。

删除桌面入口和本应用不会删除共享的 BlackHole。可先在配置页关闭登录自启动，再退出并删除应用；彻底删除个人配对和配置时移除 `~/Library/Application Support/local.DoubaoPTT.DoubaoPTT`，并在钥匙串中移除对应 `local.doubao-ptt` CA 项。不要为了卸载本软件而删除其他应用使用的虚拟音频设备。

构建脚本支持 arm64/Intel 分别打包。Apple Development 或 ad-hoc 签名仅用于开发测试；正式发行需 Developer ID 签名及公证。实际构建和实测的架构、安装流程与剩余范围分别记入 macOS 验证记录。
