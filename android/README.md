# 豆包遥控 · Android

原生 Kotlin Android App，Android 10（API 29）及以上。版本 **0.1.7（8）**。使用系统 View、AudioRecord、Camera2，网络使用 OkHttp 4.12.0，扫码使用 ZXing Core 3.5.4；无需 Google Play 服务，不是网页套壳。

首版提供开发测试签名 APK，未上架应用商店。编译、签名检查不代表已经通过安卓真机语音链路验收。

公开安装包见 [v0.2.5 Release](https://github.com/panjiang/doubao-voice-remote/releases/tag/v0.2.5)。Actions 生成未签名 APK，维护者在本机用稳定密钥签名后再公开；签名私钥不上传构建机。文件名中的 Android 版本独立于桌面版本。

## 安装与使用

1. 将 `DoubaoRemote-0.1.7-android-debug.apk` 传到手机，按 Android 提示允许本次安装来源，再安装。
2. 手机与电脑连接可互相访问的局域网；桌面端配置见[首次配置](../docs/getting-started.md)。
3. 在电脑生成现有的统一配对二维码，在 App 点击「扫码」。也可在扫码页面粘贴该二维码中的完整链接。
4. 扫码只建立本台电脑的专用 TLS 信任，不安装系统根证书，不访问二维码中的明文 HTTP 地址。配对成功后显示「已配对」。
5. 电脑光标放入目标输入框，按住顶栏以下任意位置说话。首次需允许录音权限；授权后重新按住。

按下位置出现固定直径 **180dp** 的操作圆。圆内松手完成输入并按电脑设置延迟 Enter；任意方向滑出后松手仅完成输入，滑回圆内恢复 Enter。外部波纹不参与边界判断，滑入顶栏松手仅完成输入。顶部控件不触发录音。

保留 150ms 按住门槛及准备状态；准备完成前松手取消，不迟到启动录音。Enter 是否发送消息或换行由电脑当前应用决定，等待期间再次按住或轻点会取消待发 Enter。

## 待命、权限与数据

- 松手刷新音频尾部后停止 AudioRecord，释放麦克风；再次按住重新准备。待命不采音、不保存声音。
- App 前台保持亮屏及已建立的连接、250ms 心跳；屏幕和连接仍耗电。切后台、失去活动触摸焦点、设备变化或系统暂停录音时结束本次输入。切后台会断开连接并恢复系统锁屏行为。
- 断线不自动恢复本次录音，下次按住重连。连接握手最长 5 秒，活动连接超过 1 秒无响应停止；发送队列限制约 200ms 音频，异常不回车、不重放声音。
- 只申请麦克风、扫码相机及网络权限；不申请语音识别、通讯录、电话状态、存储或后台录音权限。拒绝相机权限时仍可粘贴配对链接。
- 配对记录用 Android Keystore 中不可导出的 AES-GCM 密钥加密，存入应用的 `noBackupFilesDir`，关闭云备份和设备迁移；新配对成功后才替换旧记录。
- 支持现有协议 v1、统一二维码及旧 JSON 码。优先采集 48kHz 单声道 PCM16LE，设备不支持时尝试 44.1kHz / 24kHz，并用实际采样率握手；电脑端负责重采样。音频和 `end` 共用有序 WebSocket。

不支持后台或锁屏录音、跨公网连接，也不读取豆包识别正文。

## 从源码构建

使用 JDK 17+、Android SDK Platform 35、Build Tools 35.0.0。工程固定 Gradle 8.11.1、Android Gradle Plugin 8.7.3、Kotlin 2.0.21；Wrapper 固定官方分发 SHA-256，应用依赖锁文件随源码维护。

Windows 安装 Android Studio 及上述 SDK 后，在仓库目录：

```powershell
.\android\build.ps1 -Offline -Lint
```

`-Offline` 要求依赖和 Gradle 已缓存；首次缺少缓存时去掉此参数。脚本仅为自身进程设置 Java/SDK 路径，不改系统配置。依赖仓库优先国内镜像，随后为官方 Google Maven / Maven Central。更新依赖时可显式使用 `-UpdateLocks`，常规构建不更新锁文件。

macOS/Linux 安装相应 SDK，设置 `JAVA_HOME` 与 `ANDROID_HOME` 后：

```sh
bash android/build.sh --offline
```

产物位于 `dist/android/DoubaoRemote-0.1.7-android-debug.apk`。`assembleDebug` 和 Lint 不启动 App、录音或按键；本项目没有自动安装步骤。

Debug 签名仅用于测试。后续更新需使用相同 application ID `local.doubao.voiceremote` 和签名密钥才能覆盖安装并保留配对；正式分发应自行配置稳定的发行密钥，密钥不得提交到 Git。本次不修改现有桌面或 iOS 安装包，也不自动公开 GitHub Release。

当前版本以 iPhone 0.1.0 构建 30 为界面基准：顶栏二维码图标、配对状态、两行回车说明、0.3 秒过渡及待命停麦行为保持一致。版本号按平台独立递增。网页也采用相同固定圆手势；配对证书及常亮仍受浏览器限制。
