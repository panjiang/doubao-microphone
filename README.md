# 手机按住说话遥控豆包

iPhone 同时作为无线麦克风和远程语音快捷键。电脑端豆包负责识别及文字输入；本项目只传声音、管理按住／松开，不读取或自动发送识别结果。

桌面端是 **Rust 常驻程序＋原生托盘／菜单栏**，无 Tauri、Electron 或 WebView。手机输入与电脑配置均为网页。

## 当前状态

本研究分支已完成 Windows UMDF 虚拟键盘的本机验证：用户确认手机按住可唤起豆包、输入文字并结束。原理及排查经验见 [Windows 输入兼容性总结](docs/windows-input-findings.md)，证据和正式发布缺口见 [UMDF 实测记录](research/windows-umdf/VALIDATION.md)。这是独立研究版本，尚未合并 main，也不代表 macOS 或全部故障验收通过。下文首次使用中的 0.1.5 默认兼容模式指原版本。

这是可构建、可测试的开发实现。**模拟测试不代表已通过豆包、iPhone 或 macOS 实机验收。**逐项证据和缺口见 [验证记录](docs/verification.md)。先按 [P0 实机验证](docs/p0-validation.md) 确认模拟快捷键被当前豆包版本接受，再进行日常试用。

支持目标：Windows 11 x64、macOS 14.2+（Apple Silicon／Intel）、iOS 18+ Safari。当前没有 Linux 原生音频／按键／托盘实现，Linux 仅用于 `--mock --headless` 开发验证。

## 构建

需要 Rust 1.94.0、Node.js 22+、npm。Windows 需要 MSVC C++ 构建工具和 Windows SDK；macOS 需要 Mac 与 Xcode Command Line Tools。工程级 Cargo/npm 配置优先使用国内镜像，不改变系统配置。

```sh
cd web
npm ci --ignore-scripts
npm run build
cd ..
cargo build --locked
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

先构建网页：Rust 会把 `web/dist` 嵌入二进制，运行时不依赖 Node、npm、Vite 或外部 CDN。修改页面后需重新构建网页和 Rust 二进制。

正式构建使用 `scripts/build-windows.ps1` 或 `scripts/build-macos.sh`。Windows 有 Inno Setup 6 时生成安装包，否则输出便携 EXE 并明确提示。macOS 构建两个架构前需安装相应 Rust target；正式签名设置 `APPLE_SIGNING_IDENTITY`，公证设置已有的 Keychain `NOTARY_PROFILE`。未提供凭据时只生成标为开发测试的 ad-hoc 签名包。

## 首次使用

1. 从官方安装虚拟音频设备：[Windows VB-CABLE](https://vb-audio.com/Cable/)／[macOS BlackHole 2ch](https://existential.audio/blackhole/)。本软件不捆绑、不替换其驱动。
2. Windows 0.1.5 默认使用「豆包兼容模式」，适配默认位置安装的豆包输入法 1.0.0.23，通过现有悬浮工具栏启动和结束语音；请开启豆包的展示工具栏，并固定麦克风为 CABLE Output。该模式不模拟 Alt／Ctrl，不修改豆包程序或安装键盘驱动。豆包更新版本或布局后会拒绝触发，须重新适配。
3. 启动 `doubao-ptt serve --open`，或启动安装后的快捷方式。默认不启用登录自启动。
4. 在本机配置页选择本机局域网 IPv4、音频输出和豆包实际配置的快捷键，保存。网络地址或端口更改后需要重启程序。
5. Windows：本程序输出选择 **CABLE Input**，豆包麦克风选择 **CABLE Output**。macOS：两边均选择 **BlackHole 2ch**。若豆包没有单独选择功能，手动设置系统默认输入；这也会影响其他使用默认麦克风的应用。
6. macOS 在系统设置 → 隐私与安全性 → 辅助功能，授权主应用和 `Contents/Resources/DoubaoPTTInput.app` 输入代理。Fn 为候选快捷键，其与豆包的兼容性需实测。权限撤销后停止使用并重新授权。
7. 生成 5 分钟配对二维码，用 iPhone Safari 扫码。首次下载并安装本机证书描述文件，再在 **设置 → 通用 → 关于本机 → 证书信任设置** 开启 `Doubao PTT Local CA` 的完全信任。安装描述文件本身不等于已信任。
8. 返回向导打开 HTTPS 输入页，请求配对；在电脑配置页确认手机名称。二维码单次使用，证书流程耗时超过 5 分钟时重新生成。
9. 手机上点击“连接麦克风”并允许访问。在电脑放好光标，手机按住说话，松开后等待本次结束。

待命时麦克风保持就绪，iPhone 会显示系统麦克风指示；待命声音不发送。按“断开并关闭麦克风”可停止采集。不要开启虚拟设备“侦听”，不需要把声音播放到扬声器。

手机添加到主屏幕是可选操作。锁屏、切后台、来电或页面失焦会中止本次输入，重新连接后需再次按住。首版不保证锁屏或后台收音。

## 托盘与运行

托盘提供连接状态、运行状态、打开配置页、暂停／恢复和退出。退出时先释放快捷键，再停止音频和网络。程序运行在当前登录用户会话，不安装为系统服务。

默认端口：本机管理 `127.0.0.1:38440`；首次证书向导 `选定IP:38441`；手机 HTTPS/WSS `选定IP:38443`。只对私人网络放行后两者，不开放管理端口，不修改路由器端口映射。

Windows 手动放行示例（管理员 PowerShell，仅在确认所选网络为私人网络后执行）：

```powershell
New-NetFirewallRule -DisplayName 'DoubaoPTT LAN' -Direction Inbound -Action Allow -Protocol TCP -LocalPort 38441,38443 -Profile Private -Program 'C:\实际安装路径\doubao-ptt.exe'
```

macOS 若弹出接收入站连接提示，允许本应用。AP 访客隔离、VPN 或不同子网可能导致手机不可达；使用同一路由器，电脑可以接网线。

配置目录通过操作系统用户应用数据目录确定，启动日志输出实际路径。`admin-url.txt` 含本机管理令牌，不应分享；托盘可随时重新打开配置页。可用 `--data-dir` 指定开发目录。

## 验证与诊断命令

```sh
# 仅枚举设备和检查平台状态，不发送按键
doubao-ptt probe devices

# 三秒倒计时后按住已指定的键三秒；会产生实际键盘事件
doubao-ptt probe key --key left_alt --seconds 3

# 16-bit PCM WAV，单／双声道；需先从 devices 输出获得设备 ID 或完整名称
doubao-ptt probe audio --wav sample.wav --device "CABLE Input" --hold-key left_alt

# 无硬件模拟：不向真实麦克风输出，不产生真实按键
doubao-ptt --mock --data-dir ./artifacts/mock-data serve --headless
```

无参数启动使用托盘；`serve --headless` 在命令行运行并打印一次本机配置 URL。配置页另有明确标注的快捷键测试按钮，倒计时期间应主动切换到测试输入框。测试命令和按钮都不能自行判断豆包是否识别成功。

自动验证：

```sh
cd web
npm test
cd ..
cargo build --locked
node scripts/integration.mjs
python3 scripts/verify-dependencies.py
```

集成测试启动隔离目录中的真实二进制、可信本地 TLS、WebSocket、模拟音频和输入代理子进程；不会触发真实 Alt/Fn。`PTT_BINARY` 可指定另一个构建产物。依赖校验脚本只请求官方元数据，将镜像锁文件的校验值与官方核对，结果在 `artifacts/dependency-verification.json`，不下载或执行包。

## 数据与卸载

只持久保存设置、配对令牌的 SHA-256 摘要、本机证书及必要密钥，不默认保存录音或文字。 0.1.2 提供默认关闭的「故障录音诊断」：经用户开启后，仅在按住期间采集手机原始 PCM 和虚拟麦克风回读，保存在数据目录 `diagnostics/` 下，每份最多 30 秒，分别保留最近 3 次正常和 3 次故障样本。每份包含 `phone.wav`、`virtual-microphone.wav`（采集失败时不生成）及状态/音量时间线 `report.json`，不上传。关闭后不再录制；已有样本由用户手动删除。兼容模式每约 250ms 查询豆包采集状态，连续确认中途停止后结束本次会话并报错，不自动重新开启豆包。原生 Windows CA 私钥由 DPAPI 保护，原生 macOS CA 私钥存于 Keychain；**模拟模式仅在开发目录存文件密钥，勿用于正式配对**。服务器叶证书私钥只在内存中存在。

先退出程序，再卸载应用。Windows 安装包移除本软件配置和启动项；便携版和 macOS 使用 [卸载说明](docs/troubleshooting.md)。虚拟音频设备不自动卸载，因为可能被其他应用使用。iPhone 的本机证书描述文件需在设置中手动移除。

协议细节见 [protocol.md](docs/protocol.md)，实现边界见 [验证记录](docs/verification.md)。

### Codex 空框首次输入兼容

0.1.4 新增默认关闭的「Codex 空框兼容（先补一个空格）」。本机实测：通过豆包工具栏启动录音，在 Codex 新会话空输入框首字写入后可能触发丢弃输入并停止；同一会话追加文字、手动按住快捷键或先输入空格则正常。开关启用后，只在受支持的 Windows Codex 商店包前台、具有焦点的 ProseMirror 空编辑器内写入一个普通空格，等待 80ms 再启动豆包；已有内容不插入空格，其他程序不处理。检查只读取一个字符及文本范围端点以判断空白，不保存输入文字。开关可在配置页关闭。故障录音事件中的 codex_primed 表示本次是否实际执行了预热；该方案是应用间兼容处理，未修改豆包或 Codex 的程序文件。

0.1.5 修正空框兼容的 UIA 组件选择：使用 CUIAutomation8 支持所需的 IUIAutomation2 超时接口，避免 0.1.4 的 E_NOINTERFACE 立即失败。原生回归测试覆盖实际生产工厂初始化。
