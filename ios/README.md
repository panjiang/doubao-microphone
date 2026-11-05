# 豆包遥控 · iPhone App

使用 iPhone 麦克风，遥控电脑上的豆包输入法。原生 SwiftUI 界面配合 UIKit 触摸区域，最低支持 iOS 18，无第三方运行时依赖。

当前为开发预览，尚未提供 App Store 或 TestFlight 分发，需要通过 Xcode 配置开发签名并安装到自己的设备。电脑端安装与支持范围见[首次配置说明](../docs/getting-started.md)。

## 使用

1. 在电脑配置页点击「生成配对二维码」。
2. 打开「豆包遥控」，点击「扫码连接电脑」并扫码，无需再在电脑确认。
3. 把电脑光标放入目标输入框，在手机顶栏以下任意位置按住说话。
4. 圆内松手会结束听写并延迟按 Enter；滑出圆外松手仅完成听写，滑回圆内可恢复回车。

操作圆固定在按下位置，边界不随音量或外围波纹变化。顶部标题栏不触发录音。

桌面端 0.2.2 与 App 0.1.0 构建 23 使用统一二维码，App 和手机相机可扫描同一个码。App 同时兼容旧桌面的 JSON 二维码。App 使用内部专用证书信任，无需安装 iPhone 系统根证书；地址、CA 或凭据失效时重新扫码。

“已配对”表示已保存电脑凭据，不代表实时连接。按住时自动连接，断线后停止本次输入，下次按住再重连。首次授权时若已松手，授权后重新按住即可。

## 权限与数据

- 相机只用于扫码，麦克风用于采集声音，局域网权限用于连接电脑。
- 配对凭据保存在本设备 Keychain，首版保存一台电脑；新配对成功后替换旧记录。
- 前台首次初始化后可保持麦克风就绪，待命声音不发送、不保存。
- 切后台、锁屏、来电、音频路由变化或断线会取消当前会话，回到前台后重新按住。
- 不申请语音识别权限；识别及文字输入由电脑端豆包完成。Enter 由电脑端安排，不能代表应用已经发送消息。

## 从源码构建

需要带有 iOS 18+ SDK 的 Xcode。本仓库提供可直接打开的 `DoubaoPTT.xcodeproj`，不依赖 XcodeGen、CocoaPods 或 Swift Package 下载。

### 使用 Xcode

1. 打开 `ios/DoubaoPTT.xcodeproj`，选择 `DoubaoPTT` target。
2. 在 Signing & Capabilities 中选择自己的签名团队，首次构建使用可由该团队注册的 Bundle ID。
3. 连接并解锁 iPhone，按系统提示完成设备信任及开发者模式设置。
4. 选择目标 iPhone 构建安装。

升级已有安装时保持原 Bundle ID 和签名团队，以沿用原有应用数据与配对。开发签名有有效期，到期后需重新部署；以 Xcode 与账号实际签名状态为准。

### 命令行

在 `ios/` 目录中：

```sh
# 模拟器与未签名 iPhone 目标，只构建、不运行
bash build.sh all

# 使用自己的签名配置构建真机版本
IOS_BUNDLE_ID=com.example.doubaoremote IOS_DEVELOPMENT_TEAM=YOUR_TEAM_ID bash build.sh signed

# 首次需要 Xcode 管理描述文件及设备注册时
IOS_BUNDLE_ID=com.example.doubaoremote IOS_DEVELOPMENT_TEAM=YOUR_TEAM_ID IOS_ALLOW_PROVISIONING_UPDATES=1 bash build.sh signed

# 查看设备并安装已签名产物
xcrun devicectl list devices
bash install.sh YOUR_DEVICE_ID
```

示例 Bundle ID 和 Team ID 需替换为自己的值。安装脚本先验证签名，再安装到指定设备，不会自动启动 App 或录音。签名私钥应留在构建 Mac 上，不提交到仓库。

如在远程 Mac 上构建，需要已有可用图形登录会话及签名授权；SSH 本身不能替代钥匙串、设备解锁或系统授权。仓库的 `scripts/build-ios-remote.sh` 提供远程构建入口，其默认主机和工作目录需按自己的环境调整。

## 实现与验证

采音使用 AVAudioEngine / AVAudioConverter，输出 24kHz 单声道 PCM16LE，20ms 一帧；音频回调写入有界缓冲，转换、封包和串行网络发送在回调外处理。TLS 校验二维码指定的 CA、目标主机和证书有效期，不接受任意证书或重定向。

- [原生协议及统一二维码](../docs/native-ios-protocol.md)
- [开发与构建](../docs/development.md)
- [历史构建、安装及实测记录](../docs/verification.md)

编译、签名与安装成功不等于真机功能全部通过。扫码、权限、手势、首尾字和系统中断的验收范围以实际验证记录为准。
