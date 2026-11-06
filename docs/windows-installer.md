# Windows 安装交付

构建前检查原版 VB-CABLE ZIP、文件哈希及固定签名者。目录签名的 `.sys` 通过 Windows SDK 的 `signtool verify /kp /c <catalog> <driver>` 校验，不依赖构建机已经安装驱动或注册目录文件；因此完整安装包构建还需要 SDK 中的 `signtool.exe`。只读验证不会安装驱动或更改系统信任。参见 [Microsoft SignTool 文档](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool)。

Windows 11 x64 完整安装包内含原版 VB-CABLE Driver Pack 45，最终用户无需另行下载安装驱动。安装器首先通过 Windows WMI 按 `VBAudioVACMME` 服务查找在场的音频设备，健康则复用；设备需要重启则在安装结束时提示；设备异常或查询失败时报告原因，不盲目覆盖或卸载驱动。

设备缺少时，安装器校验归档及完整文件清单，解压全部原版文件，保持原厂名称和内容，并仅对原厂签名的 `VBCABLE_Setup_x64.exe -i -h` 请求管理员授权。校验到执行结束期间持有只读、拒绝写入的文件句柄。用户拒绝授权或安装后检测失败时，在安装准备页提示重试；不会报告为安装成功。原厂要求新装驱动后重启，即使设备已经出现也会在结束页提供重启选项，并在重启前跳过自动启动主程序。

主应用沿用当前用户安装，不要求托盘程序始终以管理员身份运行。软件卸载只移除自己的程序、启动项及原有配置，不自动卸载共享的 VB-CABLE。不修改 BIOS、Secure Boot、系统代理或驱动签名策略，也不调用系统默认音频设备切换接口。用户仍需在豆包中选择 CABLE Output，程序输出选择 CABLE Input。

安装完成时默认创建当前用户的桌面快捷方式，并通过已安装程序的专用 `install-defaults` 命令启用当前用户登录自启动，同时同步配置页中的开关。可在配置页关闭登录自启动。启动项指向安装目录的程序，参数为 `serve`，登录后仅后台运行；桌面快捷方式使用 `serve --open`。

Windows 主程序、安装器、桌面／开始菜单快捷方式和卸载列表统一采用原创蓝底白色麦克风图标，托盘使用按任务栏 DPI 选择的内嵌 RGBA 图像。生成方式见 `assets/icons/README.md`；常规构建直接使用已生成资源，需要本机 Windows SDK 提供 `rc.exe`，不增加运行时解码依赖。macOS 的图标及安装方式见 [双平台说明](cross-platform.md)。

Windows 默认自动选择语音触发方式：能打开现有 UMDF 控制接口时使用右 Alt，否则使用既有豆包兼容方式（仍受豆包版本及工具栏条件限制）。检测仅打开、关闭接口，不发送 HID 命令。普通设置只显示自动方式；其他选项在高级诊断中，启用“手动选择”并保存后生效。安装包仍不安装 UMDF 研究组件。

首次生成配置时，网络地址优先使用已连接的物理以太网 IPv4，其次是物理 Wi-Fi，再次是虚拟接口；依据 Windows 网卡类型和状态识别，不按网卡名称猜测。已保存的局域网地址不因排序变化而自动覆盖，手动修改地址后仍需重启、重新扫码。

Windows 只有一个 CABLE Input 时，配置页保存稳定的 `CABLE Input` 名称；存在多个同名设备时才保存具体端点 ID。旧 WASAPI 端点 ID 已失效时，只允许恢复到当前唯一的 CABLE Input；没有或存在多个时明确报错，不回退到系统默认设备、扬声器或 CABLE In 16ch。有效的具体设备 ID 始终优先于名称匹配。

配置页第一项提供只读运行环境检查列表，自动检测并支持单独重新检测，不改写未保存的表单。不按豆包版本号设置准入限制，缺少可确认的能力或字段时显示无法确认。说明及待人工验收步骤见 [运行环境检测](environment-checks.md)。安装阶段仍不自动启动豆包或试录音。

## 构建

```powershell
# 优先寻找 artifacts 缓存、Downloads、Desktop 中的原版归档。
.\scripts\prepare-vbcable.ps1
# 也可以明确指定构建输入：
.\scripts\prepare-vbcable.ps1 -PackagePath C:\build-inputs\VBCABLE_Driver_Pack45.zip
# 使用现有前端产物、锁定依赖离线构建，跳过测试：
.\scripts\build-windows.ps1 -SkipFrontend -SkipTests -Offline
```

`packaging/vbcable.lock.json` 固定原厂归档 SHA-256，以及 x64 安装器、Windows 10/11 驱动和目录文件的哈希及签名证书身份。准备脚本核对 Authenticode 状态后产生安装用文件清单、许可页和 Inno 常量；它不执行驱动安装、不导入证书，也不改变系统设备。缺少本地输入时明确失败，不偷偷下载或切换镜像源。

VB-CABLE 二进制及自动生成内容位于忽略的 `artifacts/vendor/vb-cable-45/`，不提交到 Git。完整安装包输出 `dist/DoubaoRemote-Setup-<版本>-x64.exe`。设备安装操作仅通过完整安装向导触发。

安装脚本必须使用 `ArchiveExtraction=full`：原厂包是 ZIP，Inno Setup 的 `enhanced/nopassword` 和 `enhanced` 仅支持 7z，会在首次安装驱动时报告 `Extraction failed: The archive format is unsupported`。该问题的历史修复与构建证据见 [验证记录](verification.md)。

## 来源与授权

VB-CABLE 来源为 [VB-Audio](https://www.vb-cable.com/)，属于 donationware，欢迎[捐赠或购买许可证](https://shop.vb-audio.com/)。依据厂商[当前分发许可](https://vb-audio.com/Services/licensing.htm)保留原版归档，允许随软件分发和静默安装，原厂身份及支持入口出现在安装向导、许可页和安装目录的 THIRD-PARTY-LICENSES.txt 中。组织／专业环境按厂商条件取得相应授权，不把个人使用条件扩大成所有场景免费。

Pack 45 原始 readme 的集成条款要求作者同意；当前官方网站明确授予满足上述条件的集成许可。本项目不修改 readme 来消除该条款，而是同时保留原文和现行许可入口。macOS 不捆绑 BlackHole；当前 Mac 安装向导在缺少设备时从官方来源获取并安装，见 [双平台说明](cross-platform.md)。

## 验证边界

本机已存在 VB-CABLE，设备在场且 ConfigManagerErrorCode=0。已校验原版 ZIP、安装器及驱动签名，完成前端、Windows Release 和 Inno 安装包编译。首次自动安装、UAC 拒绝后重试、重启后可用性以及设备异常路径仍需干净 Windows 环境实测；没有为了验证而卸载、重装当前可用的共享驱动。

UMDF 键盘属于现有独立研究组件，本次只集成音频依赖。此安装包不把 14 天研究证书或 WDK DevGen 捆绑到用户电脑；已有 UMDF 组件继续由现有配置使用，新安装实例默认使用既有豆包兼容模式。
