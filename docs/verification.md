# 验证记录

日期：2026-11-03（Asia/Shanghai）。版本：0.1.0–0.1.5，开发测试版。

## 0.1.5-umdf.1 研究分支更新

UMDF HID 右 Alt 已取得用户确认：在记事本唤起并正常结束；集成后手机按住能唤起、输入文字并结束。Secure Boot 保持开启、TESTSIGNING 关闭；安装了本机信任的研究组件，不是正式安装包验收。完整范围见 [实测记录](../research/windows-umdf/VALIDATION.md)，过滤条件、设备路径及排查经验见 [经验总结](windows-input-findings.md)。

以下保留各版本当时的验证结果；其中“尚未通过”等结论属于相应历史阶段，不覆盖本研究分支的上述进展。

## 0.1.3 连续确认录音中断

取得用户真实诊断样本：两次故障分别在 2582ms、2373ms 观测到豆包 inactive，当时面板仍 visible，手机未发 end；无诊断丢帧和回读采集错误，故障时仍有声音。第三次约 5 秒手机输入在松手后正常排空。20ms 以下采样的音量包络对比显示虚拟回读延迟约 480–490ms，相关系数约 0.98、0.97，正常样本约 0.999。用户未看到豆包错误提示。不能据此确认停止原因；0.1.2 单次 inactive 就终止的逻辑可能放大瞬时会话查询波动。

0.1.3 改为后续查询再次 inactive 且间隔至少 150ms 才中止；默认查询周期 250ms，通常需两次观测。中途恢复会清除判断计时，不自动重新触发。增加匹配豆包 PID 的捕获会话数量，以区分会话 inactive 和没查到会话。录音诊断及证书、配对设置保持。Windows Rust 19 项测试通过，新增用可控时钟验证瞬时 inactive 恢复不打断音频，以及持续 inactive 在手机仍送帧时结束会话；尚未取得本版实机反馈。

## 0.1.2 间歇性录音退出诊断

用户反馈只识别首字，状态仍为 streaming 而豆包弹框消失。现场只读检查确认 streaming、queued_ms=360，同时豆包采集会话 inactive、语音面板不可见；此前已排除用户在说话中操作电脑鼠标或键盘。根因尚未确定。

新增代理续租时查询自有豆包会话的采集/面板状态，录音提前停止时终止手机会话、保留明确错误并禁止自动重按。新增用户主动开启的本机双路诊断录音与状态时间线，30 秒滚动缓冲，正常/故障各保留 3 份。默认关闭。本次用户明确要求记录供复现核对，当前安装实例将开启。Windows Rust 17 项测试通过，前端 TypeScript 与构建通过、3 项前端测试通过。额外 0.6 秒只读采集探针确认虚拟麦克风 48kHz 回调 28320 样本、手机测试 PCM 28800 样本，采集无错误、环形缓冲丢样本为 0；探针不触发豆包、不播放测试音，样本保存在临时目录后清除。尚未取得本版真实故障录音，不能把诊断和中断反馈修复当作豆包退出根因修复。

## 0.1.1 兼容模式补充

当前 Windows 豆包输入法 1.0.0.23 的键盘处理会跳过带模拟输入标记的事件；实体按键可用，模拟右 Alt、右 Ctrl 均未唤起。两个独立进程的钩子实验也确认，在发送侧修改结构体标记没有改变接收侧的模拟标记。没有修改豆包二进制、注入其进程或安装键盘驱动。

0.1.1 增加「豆包兼容模式」：通过豆包现有悬浮工具栏的麦克风入口启动、结束语音，严格校验安装路径、版本与工具栏布局。独立输入代理继续负责租约；用每个代理唯一的窗口属性标记会话归属，父进程的故障清理只处理相应标记。

桌面验证使用单独的空白输入窗口、CABLE Output、离线生成的固定中文语音。一次完整探针记录：启动前音频采集会话 inactive，语音启动后 active；触发结束后 inactive；输入框收到 11 个字符，匹配测试短语；开始和结束均保留输入焦点。启动面板约 34ms 是该次探针数据，不能当作产品 P95。语音面板在录音停止后仍可能暂留，因此用采集会话状态和文字提交共同判断结束。

用户已确认手机音频配合实体快捷键能被豆包识别。新版兼容模式下的手机端到端体验、异常断线、两个进程分别退出、100 次／30 分钟耐久仍待实测；没有将这些项目标为通过。按用户要求，已停止追加自动语音实测，接下来由用户实际使用验证。

同日界面修订：快捷键名称按电脑端系统显示，Windows 使用 Alt，macOS 使用 Option。已通过 TypeScript／Vite 构建、Windows release 重建及更新后配置页资源检查；当前安装的 `right_alt` 配置保留。安装包和源码包同步更新。

证书向导修订：下载入口改成普通带下划线的原生 `<a href>`，使用完整绝对下载地址，不附加下载点击脚本；同时以只读文本框显示完整地址，供手机原生选择复制到 Safari。Windows 构建与实际向导响应检查通过；iPhone Chrome 长按菜单尚需手机实测。

同日 Windows 实机排查：用户确认实体右 Alt 能触发豆包，原 VK-only 模拟测试未出现面板；管理状态无输入代理错误。Windows 按键已改为 `MapVirtualKeyW(MAPVK_VK_TO_VSC_EX)` 配合 `KEYEVENTF_SCANCODE`，按下与释放使用相同扫描码及扩展标志，保留独立代理续租机制。14 个 Windows Rust 测试和 release 重建通过；本机 API 返回右 Alt 扫描码 `E038`。该方式依据 [Microsoft KEYBDINPUT 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput)。后续用户反馈仍只激活窗口菜单；临时诊断分别捕获命令行探针及配置页真实测试接口生成的事件，均为 `vk=0xA5, scan=0x38, extended=true` 的完整按下／释放，诊断事件被临时钩子阻止继续传给其他应用。双方进程完整性级别一致。**豆包接受模拟按键这一项仍未通过，具体拒绝原因未确认；不能标记听写兼容性已通过。**

## 结论

共享核心、Windows 原生端、手机网页、本机配置页、独立输入代理和安装打包代码已实现，并生成 Windows x64 EXE 与 Inno Setup 安装包。

**P0 真实听写链路尚未验收通过。**模拟测试没有调用真实按键或真实麦克风，不证明豆包接受 SendInput／模拟 Fn，也不证明 iPhone Safari 的实际采集和断线行为。

| 检查 | 结果与范围 |
|---|---|
| TypeScript 检查与 Vite 生产构建 | 通过；网页嵌入二进制，无运行时 CDN／Node 依赖 |
| Rust 格式与 Linux Clippy `-D warnings` | 通过 |
| Rust 自动测试 | Linux、Windows 原生 MSVC 各 14 个通过 |
| 前端自动测试 | 3 个通过：PCM 帧与限流、真实 AudioWorklet 源码的待命丢弃／尾帧刷新／取消 |
| Windows 原生编译检查 | 通过，固定 Rust 1.94.0、CPAL 0.18.2 |
| Windows release 构建 | 通过，生成 x64 EXE |
| Windows 安装程序编译 | 通过，Inno Setup 6.7.4；未执行产品安装／卸载验收 |
| Windows 原生设备枚举 | 通过；后续安装驱动后检测到 CABLE Input，未播放测试 WAV |
| Windows 进程／网络集成 | 通过；使用真实 release EXE、TLS 和输入代理子进程，硬件为显式 mock |
| Windows Edge 浏览器检查 | 通过；实际原生托盘运行，独立浏览器配置、合成麦克风、移动视口及触摸事件 |
| Rust/npm 镜像依赖校验 | 456 项校验值与官方元数据一致 |
| macOS 构建脚本语法 | `bash -n` 通过；不代表 Mac 构建成功 |

进程／网络集成覆盖：本机管理认证、恶意 Origin 拒绝、可信 CA 验证、配对需确认、二维码单次使用、令牌只存摘要、单手机连接、PCM 会话、输入时配置锁定、短按取消、断线、心跳超时、快速重连的旧连接隔离、输入中撤销配对、快捷键测试锁定、代理租约超时与管道 EOF。

Edge 检查覆盖：配置页读取设置、390×844 手机视口、HTTPS 配对、AudioWorklet 连续帧、触摸按住进入 Streaming、松开回到 Idle、断开关闭采集，无 JavaScript 异常、无横向溢出。使用临时浏览器配置和 `--use-fake-device-for-media-stream`，没有访问用户真实麦克风或浏览器配置。浏览器检查为局部开发证书临时忽略证书错误；可信 TLS 另由集成测试使用本机 CA 进行真实验证。

浏览器检查发现并修复了 HTTP/2 `:authority` 与 HTTP/1 `Host` 的差异；加入回归测试。截图位于本地 `artifacts/admin-desktop.png` 与 `artifacts/phone-recording.png`，均为模拟模式。

## 实机条件和待完成项

本机 Windows 11 x64 的安装记录显示豆包输入法 **1.0.0.23**，输入法服务正在运行；不将其计为兼容性通过。用户报告已安装 VB-CABLE 并重启；程序枚举确认存在 `CABLE Input (VB-Audio Virtual Cable)`，当前输出配置为 `CABLE Input`。用户实体右 Alt 测试成功，原程序按键测试未触发，扫描码修订待复验；虚拟麦克风实际听写仍未通过验收，Agent 未修改系统默认输入设备。

| 待验收项 | 当前缺口 |
|---|---|
| Windows 豆包接受模拟 Alt | 需要在专用测试输入框执行 P0 观察 |
| 手机声音 → 虚拟麦克风 → 豆包文字 | 需要安装 VB-CABLE，并连接真实 iPhone |
| iPhone Safari 前台采集、来电、锁屏、后台中断 | 未取得手机实机验证证据 |
| macOS arm64／x64 编译、Fn、BlackHole 和权限 | 无 Mac 构建／实机环境；源码和打包脚本已提供 |
| macOS 正式签名与公证 | 未提供 Developer ID／Notary Profile；未生成或声称已有 DMG |
| Windows 产品代码签名 | 开发包未签名，不是正式签名发行版 |
| 安装／卸载和真实休眠唤醒 | 未执行产品安装及实机异常验收 |
| 30 分钟、100 次真实操作 | 未执行 |
| 开始／结束 P95 ≤600／800ms | 是目标值，尚无实机统计，不标为通过 |

按照 [P0 验证步骤](p0-validation.md) 继续填写实机结果。若 macOS Fn 无法被豆包接受且没有可改绑键，应保留该平台阻塞状态，不使用私有注入接口替代。

## 构建与复现

```sh
cd web
npm ci --ignore-scripts
npm run build
npm test
cd ..
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked
node scripts/integration.mjs
python3 scripts/verify-dependencies.py
```

Windows 验证使用固定版本工具链在 Windows 本机编译；浏览器检查运行 `scripts/browser-smoke.mjs`，`PTT_BINARY` 指向 Windows release EXE，`EDGE_PATH` 可指定 Chromium／Edge。截图和机器可读依赖结果为本地产物，不加入源码历史。

Inno Setup 6.7.4 从官方来源获取，SHA-256 为 `a549929319f9120716677c685535ca4671e4b937b5fdc87e2585f32d8eb30e8c`，同时确认 Authenticode 有效，发布者为 Pyrsys B.V.。产品安装包不携带该编译器或任何虚拟音频驱动。

构建初期 Windows Rust 代理意外触发自动更新，导致检查失败。已使用经过官方 manifest 校验的国内镜像安装隔离 Rust 1.94.0，并恢复原 stable 工具链至原 1.94.0；原默认工具链名称和镜像设置未修改。后续 Windows 构建显式指向隔离编译器，没有继续依赖自动更新代理。

未创建远程仓库、提交、推送、发布版本或部署任何外部服务。

## 0.1.3 后续复现与原生跟踪

再次取得真实故障 d984c936：约 2005ms 捕获会话仍存在（capture_sessions=1）但变为 inactive，2270ms 连续确认后结束；面板仍可见，双路音量持续，未收到手机 end，诊断无丢帧。排除单次漏枚举作为这一次的解释，尚未定位豆包主动退出原因。

新增开发者工具 examples/voice_stop_trace.rs，严格限定现有默认安装路径的 1.0.0.23，临时硬件执行断点跟踪 stop、TSF stop-intent、ASR failure、no-speech timeout。使用当前调试进程权限，不修改豆包磁盘文件，不替换代码或改变分支；最多 60 秒并恢复线程调试寄存器后解除调试。1 秒连接/解除探针成功；正常手动测试的 60 秒跟踪最终 detached=true，恢复 226 个线程。工具不进入正式程序。

已捕获正常样本 d395ffa4 的结束基线：手机先 end、队列排空，随后由工具栏释放路径触发停止，原生路径含 1001/wparam=1 和 voice_stop 返回 RVA 895377、891d44。正常样本不能说明异常结束来源，ASR failure/no-speech 路径本轮尚未捕获。后续按住时可临时触发 bounded 原生跟踪，与同一时间故障录音对齐；不将尚未捕获的退出原因当成事实。

## 持续等待原生故障捕获

用户随后复现 c54ade6c（0.1.3）：2027ms 捕获会话 inactive 且仍存在，2281ms 连续确认中止，双路回读持续、无诊断丢帧。原先三分钟的临时等待已过期，故没有该次原生停止路径；不能以录音诊断代替已捕获原生根因。

新增 scripts/watch-native-stop.ps1：默认无空闲等待截止，只在当前主程序运行期间跟随按住会话，启动最多 60 秒的开发者原生跟踪；会话结束通过 stop 文件请求恢复寄存器并正常解除调试。空闲期间不附加调试器，首次捕获 doubao_stopped 故障后自行结束；退出主程序、创建 watch.stop 或跟踪错误也会结束。原生跟踪日志和对应 session_id/音频目录存于 diagnostics/native-traces，正常/故障分别保留 3 份；watch-state.json 表示 armed/tracing/fault_captured 等实际状态。脚本不注册自启动或系统服务，未并入正式安装程序。当前实例实际启动后 watch-state.state=armed、until_utc=null；录音及原生退出根因尚待一次同会话捕获。

## Codex 空框首字条件与 0.1.4 兼容处理

持续跟踪修复了 Windows PowerShell Start-Process 在未缓存 Handle 时 ExitCode 可能为空、被误判失败的问题。修复后已连续记录多次正常结束，armed 状态保持，随后捕获故障 26d21448 并自行退出，正常恢复 233 个线程并 detached=true。故障原生停止调用在约 1941ms 返回 RVA 890c84，对应 slot_SimpleMessage 内 voice discard materialized 路径，紧接着出现 1008 消息。不同于正常工具栏释放主要返回 895377；没有观测到 ASR failure 或 no-speech timeout 断点。不能仅凭未命中断点推断所有网络状态，但已经直接捕获本次主动丢弃输入导致停止的路径。

用户进一步给出稳定条件：记事本正常；Codex 新/其他会话的空输入框首次写入会中断，追加正常；空框先手动放一个空格正常；物理按住快捷键正常；手动点击豆包工具栏录音按钮也复现。根因范围确认是该工具栏入口与 Codex 初始编辑器输入会话的兼容，不是远程程序专有的点击错误；尚未证明 Codex 内部具体发生哪一种 DOM/焦点重建。只读 UIA 元数据确认 Codex 前台 PID 与焦点 PID 一致，UIA Edit、aria_role=textbox、class='ProseMirror ProseMirror-focused'，支持 TextPattern。

0.1.4 增加可关闭的空框预热开关：严格验证 Codex MSIX 应用路径、前台窗口、焦点、编辑器类和空文本范围；仅为空时以普通 Unicode 空格预热，80ms 后确认仍在同一窗口且编辑器已有内容再开始语音。使用范围端点避免将带前导空行的非空文档误判为空。结果写入 codex_primed 布尔诊断事件。默认关闭；当前用户明确复现并验证手动空格对照后，本安装实例将启用。端到端自动预热仍待用户实测反馈。

## 0.1.5 UIA 初始化回归修复

0.1.4 用户实测按住立即失败，连续样本在约 7ms 以 shortcut_failed 结束，未启动语音、未收到音频帧。只读 interfaces 探针在本机证明：legacy CUIAutomation QueryInterface(IUIAutomation2) 返回 E_NOINTERFACE (0x80004002)，CUIAutomation8 成功获得接口且 connection/transaction timeout=150ms 均调用成功。原实现组件类选择错误。

改用 CUIAutomation8 创建兼容检查组件，新增原生 Windows 回归测试直接调用生产工厂并设置所需超时；不依赖当前前台窗口，不写任何文本。启动错误完整错误链写入会话诊断，防止只保留 shortcut_failed 而丢失原因。本次 Windows 21 项测试通过，包含实际 COM 工厂回归。自动预热的最终实机结果仍待用户反馈。

## 0.1.5-umdf.2 松手 Enter

2026-11-03：增加手机松手后的单次 Enter、上滑松手仅完成听写，以及 0–5000ms 回车等待配置。沿用 UMDF 按住语音链路，不修改驱动或研究证书。动作按会话去重，处理取消、焦点变化、实体修饰键冲突，旧端缺省为仅完成听写。

本轮依用户要求不运行测试、不自动触发按键。TypeScript/Vite 构建和 Windows Release 锁定依赖离线构建完成；新手势、豆包文字落地时机和 Enter 效果交由用户实测，未计入已通过的端到端结论。

用户随后确认新版“测试过了，是可以的”，并在快速操作后再次反馈“刚才测过了，是可以的”。这是用户实测反馈，不代表所有异常路径、平台或长时间稳定性均已验收；提交时按要求未再次运行测试。
