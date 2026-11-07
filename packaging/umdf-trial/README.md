# Windows UMDF 试用组件

本地与 Actions 的 Windows 安装构建共用本目录的已签名驱动和公开证书。证书有效期至 2126-11-07，指纹为 `A1001768E45E880B77137E52ACBB00AFAEDC1894`。签名私钥保存在构建机的证书库。

驱动及安装助手源码位于 [研究目录](../../research/windows-umdf/)和 [安装助手](../install-umdf-trial.c)，驱动许可见 [LICENSE](../../research/windows-umdf/driver/LICENSE)。`umdf.generated.iss` 固定每个文件的 SHA-256，安装器在提权前校验。

修改驱动时按 [构建说明](../../docs/windows-installer.md#umdf-受控试用包)在本机重新签名，增加 INF DriverVer，更新本目录的五个文件和常量。安装助手变化时更新对应 EXE 和哈希。
