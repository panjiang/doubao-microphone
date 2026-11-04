# 语音遥控 iOS 0.1.0

原生 SwiftUI + UIKit 按住按钮，最低 iOS 18，无第三方运行时依赖。配合 Windows 桌面端 0.2.0 使用。

## 使用

1. 电脑安装 0.2.0，退出旧程序后启动新版，打开配置页。
2. 在配对区选择“iPhone App”，生成五分钟二维码。
3. 打开 App，点击“扫码连接电脑”，扫码后自动完成配对，无需电脑确认。
4. 电脑光标放入输入框，手机按住说话。首次权限授权期间若已松手，请授权后重新按住。
5. 正常松手请求电脑延迟回车；滑到按钮上边缘以外再松手只完成输入，滑回来恢复回车。左右滑出不改变动作。

无需安装系统根证书，扫码 CA 仅作为本 App 内这台电脑的信任锚。地址或 CA 改变、凭据撤销时重新扫码，新配对成功前不覆盖旧记录。凭据保存在仅本设备使用的 Keychain。

前台第一次按住后保持采集就绪，待命声音不发送、不保存；进入非活动状态、锁屏、后台、来电、路由变化或断线均停止，返回前台后必须重新按住。App 不读取豆包识别结果，声音结束不代表文字已经落地，Enter 仅表示向电脑发送回车。

## 构建

可直接打开 `DoubaoPTT.xcodeproj`，无需 XcodeGen、CocoaPods 或 Swift Package 下载。Mac 上执行：

```sh
bash build.sh all # 仅构建模拟器和未签名真机包，不运行测试
```

从当前工程根目录远程构建：

```sh
bash scripts/build-ios-remote.sh all
```

同步目录固定为 `/Users/houzzkitci/codex-builds/doubao-ptt-ios`，不删除或同步其他项目；产物回传 `dist/ios/`。图标用 CoreGraphics/ImageIO 在无图形会话环境生成。

## 签名和安装

在 Mac 上选择你已授权的开发团队，私钥留在 Mac：

```sh
IOS_DEVELOPMENT_TEAM=你的TeamID bash build.sh signed
# 首次缺少本 App 描述文件时，允许 Xcode 管理开发签名：
IOS_DEVELOPMENT_TEAM=你的TeamID IOS_ALLOW_PROVISIONING_UPDATES=1 bash build.sh signed
xcrun devicectl list devices
bash install.sh 目标手机的设备标识
```

安装脚本先验证签名再安装，不启动 App、不申请麦克风权限、不触发电脑输入。手机需配对、解锁并满足开发者模式要求。免费 Personal Team 通常七天后需重新部署，以 Xcode 实际生成的期限为准。

若 SSH 下报 `User interaction is not allowed`，不一定是钥匙串未解锁。本机已确认同一用户的图形登录会话可访问钥匙串：通过临时用户 LaunchAgent（`launchctl bootstrap gui/501 ...`）在该会话运行 `build.sh signed` 后签名成功，结束后已卸载临时任务。该办法要求已有可用的图形登录会话和授权开发账号；若仍要求真实密码或账号验证，需完成正常授权。没有修改钥匙串访问策略、导出私钥或复用其他 App 的 Bundle ID／描述文件。

## 实现边界

- 协议 v1，24kHz 单声道 PCM16LE，20ms/帧；24 字节头为 PPT1、UUID 原始十六字节、序号 UInt32LE。
- 音频回调仅向 C11 原子有界环形缓冲写入平均后的单声道采样。转换、封包和发送在回调外完成；原始缓冲约 500ms，活动会话积压超过 200ms 中止。
- 发送预算在进入串行队列前计算，最多 4800 个待发采样（200ms），包括排队和发送中的音频。发送等待超过 200ms、心跳一秒未收到响应时关闭连接，不重播中断音频。
- 相机只在扫码页运行。无后台音频、语音识别权限、第三方分析或识别结果读取。
- TLS 校验扫码 CA、证书链、有效期及地址，拒绝重定向和不匹配证书；ATS 仅允许本地网络，没有全局任意加载开关。

## 验证记录

真机验收应覆盖扫码自动配对、重新打开 App、权限弹窗期间松手、正常松手单次 Enter、上滑／滑回、快速重复、待发 Enter 取消、首尾字、后台／锁屏／来电／断网取消，以及电脑撤销配对和原网页继续使用。检测按钮和模拟模式不能代替这些验收。

模拟器 arm64/x86_64 与 iPhone arm64 目标构建通过。未运行自动测试、模拟器交互、录音或自动按键。真机配对、权限弹窗、手势、首尾字、快速重复和系统中断仍待用户实测。

2026-11-04：已通过 Mac 图形登录会话生成本 App 的开发描述文件并完成签名，`codesign --verify --deep --strict` 通过。Bundle ID 为 `local.doubao-ptt.ios`，团队 `F4ZQ3W6CN3`，描述文件包含目标“X”（iPhone 15 Pro），到期时间为 2027-11-04 06:58:41 UTC（以实际签名及设备要求为准）。首次安装被设备锁屏阻止（`kAMDMobileImageMounterDeviceLocked`）；用户解锁后已通过 `devicectl` 成功安装到“X”。未启动 App、录音或触发电脑按键，真机功能仍待用户验收。

图标更新自 0.1.0 构建 2 起，蓝色背景铺满画布、外形由 iOS 裁切。当前构建 7 顶部仅显示“未配对／已配对”，中央声波提高视觉响应和起伏幅度；`dist/ios/signed/` 与 signed ZIP 为构建 7，此前 unsigned 产物仍为构建 1。

状态说明：顶部配对状态取决于是否保存电脑凭据，与实时连接无关。按住时自动连接，断线后停止本次会话，下一次按住重新连接。连接握手最多等待 5 秒，已连接后一秒无响应或发送超时会在页面显示错误；待命不自动重连、不恢复旧录音。首次连接的 TCP／TLS／WebSocket 握手不使用活动会话的 200ms 发送上限。
