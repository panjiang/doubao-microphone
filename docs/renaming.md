# 豆包远程：名称和覆盖升级

中文简称“豆包远程”，全称“豆包输入法远程控制”；英文简称 Doubao Remote，全称 Doubao Input Method Remote Control。电脑和手机都是这个显示名称，文档和下载页再区分电脑端／手机端。

仓库、Rust 包及桌面命令为 `doubao-remote`，Rust crate 为 `doubao_remote`。iOS 工程及应用产物是 `DoubaoRemote`。macOS 只安装一个主应用，按键助手内置在主应用包中，不单独分发或安装；系统辅助功能设置可能单列这个助手。

统一命名首批产物为桌面 0.2.6、iOS 0.1.0 构建 31、Android 0.1.8 构建 9；后续深灰主题的手机版本以各端最新构建清单为准。v0.2.6 汇总统一命名与深灰主题，iOS 为构建 32、Android 为 0.1.9（10）；原 v0.2.5 及此前附件保持原名称与哈希。网页、音频、手势、Enter 和设备配置的实际行为保持原设计。

## 保留的兼容标识

| 标识 | 保留原因及位置 |
| --- | --- |
| Windows AppId `{BCF882D6-2071-4E38-A957-54E5805439F7}` | 识别覆盖安装，更新同一个卸载项 |
| Apple `local.doubao-ptt.*` Bundle ID | iOS 覆盖安装、Keychain 可访问性及 macOS 系统身份；不更换签名团队 |
| Android `local.doubao.voiceremote`、原 Keystore alias 和测试签名 | 覆盖旧 APK 并读取配对记录 |
| 网页 `ptt-device-token`、`ptt-admin-token` | 保留当前 Origin 下的手机配对和管理凭据 |
| `DoubaoPTT` 旧数据目录、旧安装目录及自启动名称 | 读取现有配置与 CA；安装器在新程序配置成功后清理旧入口，不迁移凭据文件 |
| `local.doubao-ptt` Keychain CA service | 读取旧私钥；账户名由原数据目录派生，目录保持原位 |
| 已有 CA 的 Subject／SKI 及 `local.doubao-ptt.ca/profile` 描述文件标识 | 旧根证书及私钥保留；服务器证书按原 CA 参数签发。新 CA 使用 Doubao Remote Local CA |
| `doubao_ptt_pair`、`PPT1` 以及 C 层 `ptt_*` 接口 | 已有二维码、音频协议和原生接口；PTT 在这些接口中指按住说话，不作为产品名称 |
| UMDF 描述符、硬件 ID、驱动文件及 `DoubaoPTT.VoiceOwner.*` | 保持当前安装的键盘与互斥协调可用，不改签或重装研究驱动 |

研究驱动目录、历史 Release 说明及验证记录保留原始名称，不把历史产物改写成新产物。

## 数据目录和自启动

默认选择顺序为：显式 `--data-dir` → 已有有效新目录 → 已有有效旧目录 → 创建新目录。包含 settings.json、devices.json、ca.pem 或已有 CA 私钥文件即视为有效数据；读取错误直接报告，不绕过旧目录创建新身份。新旧均有效时优先新目录，不合并、覆盖或删除另一份。

全新安装使用 `DoubaoRemote` 平台数据目录；旧安装继续使用 `DoubaoPTT` 目录。macOS 依次读取新旧 Keychain service，只在两者都不存在且没有原 CA 证书时创建新私钥；原 CA 无法解析、证书与私钥不匹配或 Keychain 拒绝访问时，保留原件并报错。

新 Windows 启动项名为 `DoubaoRemote`，Mac LaunchAgent 为 `local.doubao-remote.plist`。新装默认启用自启动，升级保留原开关。旧启动入口在新设置成功应用后清理，不留下两个启动项；在自定义数据目录下仍不建议启用登录启动。

## 覆盖安装和验收

先从托盘／菜单栏退出旧程序，再覆盖安装。Windows 复用原安装目录及 AppId，配置失败时恢复原程序、设置和启动项；成功后清理旧 EXE 和已知旧快捷方式。Mac 在原应用退出后，将已验签的新包暂存，再备份并替换新旧应用路径；替换失败恢复原路径。桌面只保留本产品的新入口，不清理无关文件。

升级后人工确认：应用名称和快捷方式、仅一个应用入口、自启动开关、原手机配对、证书信任、虚拟麦克风及 UMDF 识别。macOS 辅助功能权限可能需要重新确认，以系统实际提示为准。统一命名阶段仅构建与静态检查；后续深灰主题已在当前 Windows 覆盖升级并只读确认原设置、配对和 CA 保留，iOS 构建 32 已安装到指定 iPhone。其他安装及回滚场景仍需用户验收，未自动录音、触发按键或运行测试。
