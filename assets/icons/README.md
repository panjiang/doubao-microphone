# DoubaoPTT 图标

原创圆形蓝底白色麦克风，遵循本项目 MIT 许可。背景色为页面使用的 `#234bc6`，圆形之外透明，无第三方图案或字体。

`doubao-ptt.svg` 是矢量图形；`doubao-ptt.png` 为 256px 预览；`doubao-ptt.ico` 包含 16、24、32、48、64、128、256px PNG 帧。`tray.svg` 与 `tray-32.png` 为托盘图预览。圆形直径占满画布，麦克风四周保留约 12%–16% 的画布宽度作为内边距，避免贴边。

`tray-{size}.rgba` 是 16/20/24/28/32/40/48/64/128px、从左上逐行排列的 RGBA8 非预乘透明像素。托盘按主任务栏 DPI 和 `SM_CXSMICON`/`SM_CYSMICON` 选择图像，DPI 变化后在后续事件循环更新；不常见尺寸由系统从 128px 源图缩放。Windows 固定托盘显示尺寸，单个程序无法突破该格子大小；大图源文件本身不意味着屏幕上会显示得更大。

重新生成（仅使用 Windows 自带的 PowerShell 和 System.Drawing）：

```powershell
.\scripts\generate-icons.ps1
```

SVG 和 GDI+ 绘图坐标均保存在生成脚本中；修改图形时同步两者并重新生成。生成文件随源码提交，常规构建无需重新生成、联网或增加图片解码依赖。

Windows SDK 的资源编译器 `rc.exe` 在构建时将 ICO 嵌入主程序；支持通过 `RC` 环境变量指定编译器。安装包读取同一 ICO。macOS 图标本轮保持原实现。
