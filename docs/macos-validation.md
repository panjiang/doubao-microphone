# macOS 实机验证记录

## 2026-11-04：Fn 触发及虚拟麦克风链路通过

本次由用户远程观察 Mac 桌面并确认结果，不以系统 API 返回成功或会话完成消息代替豆包响应。

| 项目 | 实际环境 |
| --- | --- |
| 电脑 | Mac mini，Apple Silicon / arm64 |
| 系统 | macOS 26.5.1，25F80 |
| 构建工具 | Xcode 26.5，Rust 1.94.0 |
| 源码 | main，f60a885；116 个非 iOS 跟踪文件及 7 个网页构建文件与远端一致 |
| 程序 | DoubaoPTT 0.2.0，Apple Development 开发签名 |
| 输入法 | 豆包输入法 1.0.3，CFBundleVersion 1 |
| 虚拟设备 | BlackHole 2ch 0.7.1，CFBundleVersion 596 |
| 输出及输入 | 程序输出到 BlackHole 2ch；豆包从 BlackHole 2ch 接收 |
| 快捷键 | Fn，由已获辅助功能权限的独立输入代理通过 CoreGraphics 模拟 |

测试不依赖电脑的真实麦克风。使用本机已有 Tingting 声音生成固定中文测试语音，转换为 24kHz、单声道 PCM16 WAV，通过临时配对的 WSS 客户端发送给运行中的正式会话控制器，再输出到 BlackHole。TLS 使用本机 CA 验证，未关闭证书校验。控制和音频使用现有协议；结束动作使用 finish，不发送 Enter。

测试位于专用 TextEdit 文档 `dist/macos-voice-validation.txt`。程序发送 Fn 按下、215 帧测试声音及 end，完成排空后释放 Fn。用户明确确认：“豆包录音面板，已输入文字并停止录音”。会话 ae474dec-5400-4ad4-be0d-87a2c9274d3e 返回 ready 和 complete=true、reason=completed，发送到结束确认约 4858ms，包含约 4.3 秒测试声音；不是启动延迟或 P95 测量。

结论：当前机器、权限和豆包版本下，CoreGraphics 模拟 Fn 可以实际唤起豆包、通过虚拟麦克风识别文字，并在释放后正常结束。本次无需引入虚拟 HID 键盘；此结论不推及其他豆包或系统版本。

测试客户端已断开，临时配对已撤销。检测到设备列表为空、程序处于 disconnected/空闲，无错误、无测试任务运行。

## 构建及测试包

- arm64 Release 构建通过；锁定依赖离线构建，20 个单元测试通过。
- 主应用与嵌套输入代理签名校验通过；DMG 校验与挂载检查通过。
- 测试包：`DoubaoPTT-0.2.0-macos-arm64-test.dmg`。
- SHA-256：`aaeffa8a7244031c2a1c1ba014a35263048af96bb93bda717373d44e47f1574a`。
- Mac 产物目录：`/Users/houzzkitci/codex-builds/doubao-ptt-macos/dist`。
- 本地归档目录：`dist/macos`。

这是 Apple Development 签名的开发测试包，未进行 Developer ID 签名和公证，不作为正式分发包。

## 0.2.0 测试时的未完成项（后续进展见下文）

- iPhone App 或 Safari 到这台 Mac 的实际采音链路。
- Intel 构建和实机验证，其他编辑器、首尾字准确性、异常释放和 30 分钟/100 次耐久验收。
- macOS 松手延迟 Enter 的平台适配（当前握手 capabilities 为空），环境检查列表、应用/菜单栏图标、设备与触发选项简化以及默认自启动的功能对齐。

目前仅将 Fn 触发、虚拟音频输入和正常释放标为通过，不将 macOS 完整 P0 或 Windows 功能对齐标为完成。

## 2026-11-04：0.2.1 双平台功能对齐

源于同一 main 的 f60a885 加本次未提交改动，未创建另一套桌面项目、未提交或推送。复用同一配置页、协议、会话/音频状态机和 Enter 调度；macOS 补充原生 Enter、焦点保护及只读环境适配，详见 [共用逻辑说明](cross-platform.md)。

已实现：

- 配置页显示相同的运行环境检查列表、loading、失败/未知状态、重新检测和回车等待；Mac 额外检查当前输入法和主程序/代理辅助功能权限。
- Mac 只显示 BlackHole 2ch，默认自动 Fn，高级诊断中可手动选择；网络接口已正确识别 en0 为以太网并优先排列。
- Mac 主程序、输入代理和原生安装器接入圆形蓝底麦克风 ICNS；菜单栏替换蓝点为麦克风图像。
- 正常完成后按设置等待，再检查原前台应用、窗口、焦点和修饰键，发送固定 Return；等待、取消和结果事件使用原有共享逻辑。
- 已安装应用首次启动默认启用登录自启动，已有配置选择保留；当前 Mac 的配置及 LaunchAgent 已确认开启。
- DMG 内增加原生安装向导。缺少 BlackHole 时优先缓存，否则从官网获取原版、检查哈希/发布者/系统评估并通过系统管理员认证安装；创建桌面入口。未重复安装或卸载本机已有 BlackHole。

### 实际验证

| 项目 | 结果与范围 |
| --- | --- |
| 前端 | TypeScript/Vite 构建通过 |
| Mac 单元测试 | 21 通过，0 失败；含新增设置存储 CRC、重复值、删除值和越界检测 |
| Mac 原生构建 | arm64 Release 和 Clippy `-D warnings` 通过；Swift 原生安装器编译通过 |
| Windows 回归编译 | 独立目录中 `cargo check --locked --offline --all-targets` 通过；未替换 Windows 运行程序 |
| 环境实际读取 | BlackHole、豆包进程、Fn 编码、语音开关及两项权限通过；自动麦克风正确显示 unknown，未将其当作明确选择 BlackHole |
| 正常 Enter | 会话 579f5a49-c14b-4870-bec5-e5d43c23e5c9，正常 ended 后约 1040ms 返回 executed/enter_sent；end 到快捷键释放确认约 253ms |
| 等待时取消 | 会话 988276fe-3579-4c02-845f-5983f7f7fc6e，返回 skipped/cancelled |
| 同应用切换窗口 | 会话 3b87d106-61c5-4deb-b9ab-96a881ac82c3，切换到另一个专用 TextEdit 文档后返回 skipped/focus_changed |
| 等待时再次开始 | 会话 55354175-07b2-45c7-a135-b2daa87a2aa6 返回 skipped/new_session；第二会话正常 ready，并在测试取消后 ended，不安排 Enter |
| 包完整性 | 安装器、嵌套主应用和输入代理的签名校验通过；DMG 校验及只读挂载通过；运行主程序与包内主程序 SHA-256 一致 |

上述动作测试通过原生 WSS 入口、专用 TextEdit 文档和固定合成语音执行，TLS 验证开启，临时配对均已撤销；没有向其他用户文档发送按键。action_result 表示系统事件已发出或已跳过，不代表读取到应用已发送消息。实际换行已于 2026-11-05 获用户确认，见下文；iPhone 到 Mac 实测尚待补充。

初始焦点实现通过系统级 AXFocusedApplication 查询时收到 -25204，按保护规则跳过 Enter；诊断后改为 NSWorkspace 取得前台 PID，再查询该应用的 AX 窗口和焦点。每次最多两次 80ms 远程属性查询，并复核前台 PID；未通过关闭焦点保护解决问题。改动后正常发送和换窗口跳过均通过。

### 交付

- 包：`DoubaoPTT-0.2.1-macos-aarch64-test.dmg`，Mac 桌面和本地 `dist/macos/` 各一份。
- 包 SHA-256：`deb24d485f827e62ad1f898ba01c7841c9f4afb21cad115a59a3a8ed5ebe9c03`。
- 已安装主程序 SHA-256：`688e0f9e95d4ee1b40bfd188a56901fbd852bf9baf174d89714ab8460ace730f`。
- Mac 已替换并启动 0.2.1，保留原数据目录与 CA；桌面入口及新的本机配置页已打开。打包临时 LaunchAgent 已移除。
- 使用 Apple Development 签名，未进行正式 Developer ID 公证。

剩余实测：iPhone App/Safari 到 Mac、安装向导首次缺少 BlackHole/授权取消/断网分支、实际登录启动、不同控件焦点切换、两个进程故障释放、Intel 构建和真机，以及 30 分钟/100 次耐久。未为验证首次安装而删除共享的虚拟设备，也未将这些项目标为通过。

## 2026-11-05：用户确认实际 Enter 换行

按用户要求再次演示，在原专用 TextEdit 文档中发送固定测试语音并以 enter 正常结束。会话 `5b78dc09-874d-4882-9612-28e67bfb7e23` 正常 ready、ended（complete=true），结束到快捷键释放确认约 288ms，随后约 1024ms 返回 executed/enter_sent。用户明确确认“已自动换到下一行”。因此，Mac 0.2.1 在该文档的语音输入、正常结束以及延迟 Enter 实际生效均已验证，不再只是按键事件回执。临时配对已撤销，本次没有代码或安装包变更；不将此结果计为 iPhone 真机链路或其他未测项目通过。
