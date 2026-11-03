# UMDF 本机验证记录

2026-11-03，Windows 11 Pro `10.0.26200`，微星 PRO B850-P WIFI。用户授权允许 Windows 系统配置变更，禁止修改 BIOS；只接受纯软件。

## 已执行与实测

| 项目 | 结果 |
| --- | --- |
| 架构 | 自己的 UMDF 2.33 DLL + 系统 MsHidUmdf／WUDFRd；不安装自研内核 SYS |
| 参考来源 | 微软 vhidmini2 源码，逐文件 SHA-256 见 `reference-sources.json`；源码及改编驱动保留 MS-PL 许可，未集成 HIDMaestro SDK |
| 构建 | 复用已核对官方 SHA-512 的 WDK 26100.6584；MSVC 编译通过，InfVerif `/w` VALID，Inf2Cat 无错误／警告；驱动自签名 |
| 加载前提 | 安装前 Secure Boot=true、TESTSIGNING=false；安装后仍相同，未重启，未改 UEFI／BCD／VBS |
| 证书 | 仅导入本研究的代码签名证书到 LocalMachine Root、TrustedPublisher；专用签名私钥在签名完成后删除，不导入系统私钥库 |
| 设备 | 已启动，ConfigManagerErrorCode=0，驱动栈为 mshidumdf／WUDFRd／PnpManager |
| 控制接口 | `umdf-probe status` 返回 version=1、online=1、held=0 |
| 右 Ctrl 首次试验 | 事件 scan=0x1d、extended=1、injected=0；不是豆包当前配置键，不作为豆包兼容性证据 |
| 当前豆包设置 | 只读核对 `voiceLongPressShortcut.modifierFlags=2049`，即右 Alt；`enableGlobalVoiceShortcut=false`，未修改豆包配置 |
| 右 Alt 首次试验 | 正确键值与扫描码、injected=0，但用户报告未出现面板；当时未明确建立文本输入焦点，不声称兼容成功 |
| 实体键对照 | 用户在记事本输入区用实体右 Alt，确认能唤起 |
| 明确焦点后的右 Alt | 等待 8 秒供用户切回记事本；UMDF 按住约两秒，用户确认“出现了，并正常结束” |
| 超时释放 | `expire right-alt` 不续租，驱动自动产生未注入的释放事件，最终 held=0、expired=1、release_errors=0 |
| Rust 手机端集成 | 复制当前 main 的 0.1.5 工作源码到独立 worktree；来源文件哈希见 `app-source-base.json`；增加明确的 `hid_right_alt` / `hid_right_control` 选项，无 SendInput 回退 |
| 构建／自动检查 | Rust Windows Release 离线构建通过；21 项库测试通过；TypeScript/Vite 构建、rustfmt 和差异空白检查通过 |
| 运行测试版 | 已切换到 `0.1.5-umdf.1`，独立 UMDF-Test 数据目录；设置为 `hid_right_alt`，沿用 IP、端口、手机配对和 CA；原安装 EXE／设置未覆盖 |
| 手机按住完整流程 | 用户明确反馈“能唤起、输入文字并结束”，本机端到端正常流程通过 |

## 事件证据的边界

右 Alt 观察到：`scan=0x38 extended=1`，按下及对应释放均为 `injected=0`。两秒保持期间观察器收到约 48–49 个 down 和一个非注入 up，包含重复 down；未把它们计作多个按住会话。随后另有一个 `injected=1` 的额外 up，来源未归属，未将整段日志描述为全部无注入事件。UMDF 原型及本机客户端都没有调用 SendInput。

额外 up 没有阻止这次用户确认的正常唤起和结束，但正式版应继续核对实体键与软件共同使用的行为。没有截图、读取识别正文或自动语音识别结果验证；豆包面板表现以用户报告为准。

## 当前安装与精确回退

- 硬件 ID：`ROOT\DOUBAOPTT_UMDF_RESEARCH`。
- 设备实例：`ROOT\DEVGEN\{0EE19B18-153A-0F42-899F-C4ED7B60D68B}`。
- 驱动包：`oem52.inf`，原名 `doubaopttum.inf`，Provider `DoubaoPTT Research`。
- 证书 SHA-1 指纹：`DF56329FB62C70A339B56E844D4EBA60F95AE8DC`。14 天研究证书，到期需更新；不是长期发布包。
- Windows 研究目录：`C:\Users\Administrator\AppData\Local\Temp\doubao-ptt-umdf-research`。
- 只读状态：`tools\install-p0.ps1 -Action Inspect`；移除：`tools\install-p0.ps1 -Action Remove`，会验证 ledger、设备和 OEM 包归属，只移除本次新增的两项证书信任。
- ledger：研究目录的 `out\installation-ledger.json`。
- 恢复原程序：研究目录的 `tools\switch-trial.ps1 -Restore`；它只允许在空闲时切换，并重新启动原安装 EXE 和原数据目录。

测试版主程序在另一独立构建目录 `C:\Users\Administrator\AppData\Local\Temp\doubao-ptt-umdf-app-build\target\release\doubao-ptt.exe`。默认程序入口仍为原安装位置；本轮没有更新自启动注册表或系统安装器。

## 正式交付前的剩余工作

- 手机正常输入已有用户成功反馈；首尾字的专项对照与长期排空稳定性仍待验证。
- 普通权限用户设备访问验证、受限代理及会话所有权；当前本机研究进程继承管理员身份，不能冒充普通用户测试完成。
- 客户端／输入代理／UMDF 宿主异常、电源和移除、物理同键冲突及连续操作。当前两个进程和宿主的全部故障场景未实测。
- 发布级私钥与更新策略、证书信任告知、持久目录 ACL、回滚和卸载幂等性。研究签名 14 天不能作为正式使用策略。
- 用自己的安装组件替代 WDK DevGen；DevGen 不允许用于生产分发。
- P0 驱动可用的 HID 功能范围比产品必要范围更大；正式组件还需收敛描述符／样例处理逻辑并审查控制接口权限、超时和同步调用阻塞。
- macOS 尚未编译／实测，UMDF 不适用于 macOS。

这些限制不影响已证实的结论：**这台电脑在 Secure Boot 开启、不购买 EV 证书、不修改 BIOS 的条件下，已经加载并通过 UMDF 产生了能被豆包接受的右 Alt。** 它尚不等于安装器、所有故障边界和批量分发已验收。
