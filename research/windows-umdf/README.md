# 手机遥控豆包：纯软件替代方案

日期：2026-11-03。独立 worktree：`/home/jiang/ksf/repositories/doubao-ptt-umdf`，分支 `research/windows-umdf`。从已有源码基线 `b4b7874` 分出；原 KMDF/VHF 研究保留在另一 worktree，不继续作为当前产品路线。

## 用户约束与本轮结论

用户拒绝自研内核驱动的签名成本和修改 BIOS 的安装方式，并明确只接受纯软件。保留 iPhone、现有音频链路和电脑端豆包，不增加 USB 硬件。

新的首要验证候选为 **UMDF 2 用户态 HID minidriver**。它与前一版 KMDF/VHF 源驱动的架构不同：本项目提供用户态 DLL，使用 Windows 自带的 MsHidUmdf.sys 及 HID/键盘组件。微软明确支持这类实现，并提供 vhidmini2 示例。[微软架构文档](https://learn.microsoft.com/en-us/windows-hardware/drivers/wdf/creating-umdf-hid-minidrivers)、[官方示例](https://github.com/microsoft/Windows-driver-samples/tree/main/hid/vhidmini2)

**已完成本机 P0 和手机完整链路：用户确认右 Alt 能唤起豆包；切换到 UMDF 测试版后，手机按住能唤起、输入文字并正常结束。** Secure Boot 开启、TESTSIGNING 关闭，未重启。仍不能承诺所有受管电脑都允许安装。详细证据、设备标识及剩余限制见 [验证记录](VALIDATION.md)。

“豆包跳过带注入标记的语音快捷键事件，而非全局屏蔽所有虚拟键盘”的原理、证据边界及排查经验已整理在 [Windows 输入兼容性总结](../../docs/windows-input-findings.md)。

## 为什么继续研究这一候选

HIDMaestro 的源码、文档说明其使用 UMDF2 提供虚拟键盘／鼠标，并通过本机信任的代码签名证书安装，不启用 TESTSIGNING，也不要求自建内核驱动的 EV 签名流程。主项目采用 MIT 许可，可作为实现研究和最小实验候选；不是微软官方背书或成熟度保证。[项目源码](https://github.com/hifihedgehog/HIDMaestro)、[许可证](https://github.com/hifihedgehog/HIDMaestro/blob/main/LICENSE)、[安装与签名说明](https://hidmaestro.org/docs/reference/driver-install-and-signing/)

这里有明确代价：**用户态组件仍是驱动；本机签名证书的安装属于系统信任变更。** 不能把它描述为“完全免驱”“无需任何系统改动”，不能静默处理成用户不知情的安装。该实现还会将私钥保存在本机证书库，这一行为及其 ACL、升级、卸载需审查；不直接照搬到正式产品。

若后续选用受信任 CA 证书而非本机证书，则相应签名成本需要重新评估，不能继续宣称没有证书费用。用户进一步明确“可以改系统配置，但不是改 BIOS”，在已说明本机证书与用户态组件安装内容后，已按此授权安装研究组件。没有注册、购买或付费签名。

## 目标架构与安装体验

```text
iPhone 按住／松开 + 声音
  → 现有 Rust 会话核心
  ├─ 音频 → 现有虚拟音频设备 → 豆包
  └─ 受限本机输入代理
       → UMDF 用户态 HID 组件
       → Windows 自带 HID／键盘栈
       → 豆包右 Alt 或右 Ctrl
```

普通用户安装时只使用安装程序，并批准明确列出的管理员操作；Secure Boot 保持开启，不进入 BIOS、不设置 TESTSIGNING、不关闭内存完整性。若验证方案需要安装本机代码签名证书，必须在安装界面说明用途、指纹和移除范围。允许系统按设备安装实际情况要求重启，不能预先保证永不重启。

本机证书、驱动研究安装已执行并记录精确回退标识；原已安装程序未覆盖。测试版使用独立数据目录，复制原证书及配对以沿用手机地址，设置为 `hid_right_alt`。

## 最小实测门槛

1. 固定依赖的正式版本和源码 SHA；审查许可证、安装器、证书存储及 ACL。仅选标准键盘路径，不启用游戏手柄、设备克隆或额外 USB/IP 内核组件。
2. 优先复用已校验的本地 WDK/SDK；若候选需要新的大型 .NET 或发行包，先查缓存及可信国内镜像，公布下载量并核验官方 hash／签名。
3. 准备仅含右 Alt／右 Ctrl 的 P0 工具和安装清单。首次试验必须保持当前 Secure Boot 开启、TESTSIGNING 关闭，不改变现有手机程序或豆包配置。
4. 获准安装后，确认用户态 DLL 及系统自带组件的实际驱动栈；核对设备确实产生键盘事件，而不是退回 SendInput。
5. 观察右键值、扫描码、extended、LLKHF_INJECTED 和 down/up；由用户报告豆包是否正常开始与结束。两类证据都通过才算打通。
6. 验证超时释放、客户端退出／挂起、用户态宿主退出、休眠、卸载和实体同键冲突。确认故障后不自动重按；锁屏／会话改变时停止。
7. 通过后再集成进原 Rust 输入代理，保留普通权限托盘，通过受限代理取得需要的设备权限；网络协议仍只表达开始／结束听写，绝不开放任意按键执行。

失败条件包括：需要关闭 Secure Boot、只能退回 SendInput、仍被豆包忽略、本机证书安装不被接受、运行故障释放不成立。遇到这些条件就保留失败证据，不把候选直接写成正式版本。

## 已排除或保留的其他路线

- **现有无键盘模拟的豆包按钮兼容模式：** 已有用户实测可用，继续保留。当前实现依赖指定豆包版本与布局，不冒充稳定公开 API；应做版本适配和明确诊断。
- **商用虚拟设备 SDK：** 可降低自购签名门槛，但授权、每机费用、实际键盘支持和分发权仍需要核对，不直接认为“现成即免费”。
- **Interception：** 官方仅列测试到 Windows 10，商业用途另有许可；不选作 Windows 11 当前默认路线。[官方说明](https://github.com/oblitum/Interception)
- **改用其他识别服务：** 会改变由电脑端豆包负责识别与输入的产品目标，本轮不擅自替换。
- **USB 接收器：** 用户已明确排除。

## 平台边界

这条替代方案针对 Windows。现有共享音频和网页模块仍可复用，macOS 快捷键／输入适配需单独实测；UMDF 不能带来 macOS 支持，也不能据 Windows 资料声称双平台功能完成。
