# 协议与并发约定

版本 1，仅局域网 WSS。同一时刻仅允许一个已配对手机。TLS 握手之后，5 秒内发送 `hello`；令牌只出现在加密连接的消息体中，不放入 WebSocket URL。Origin 和 Host 必须与选定的手机 HTTPS 地址一致。

```json
{"type":"hello","version":1,"token":"配对后得到的秘密","sample_rate":48000,"channels":1}
{"type":"start","session_id":"UUID v4"}
{"type":"end","session_id":"UUID v4","last_seq":19}
{"type":"cancel","session_id":"UUID v4"}
{"type":"heartbeat"}
```

服务端回应 `hello`（包含 `mock`）、`ready(session_id)`、`ended(session_id, complete, reason)` 或 `error(code,message)`。`complete=true` 表示传输、尾部排空和快捷键释放完成，不表示已读取或确认豆包识别结果。

音频每帧约 20ms，单声道 PCM16 小端。采样率 8000–96000Hz 且可被 50 整除，采用浏览器实际采样率。最后一帧可以不足 20ms；没有音频时 `last_seq=null`。

| 字节 | 内容 |
|---|---|
| 0–3 | ASCII `PPT1` |
| 4–19 | UUID 的 16 个原始字节，按文本顺序解码 |
| 20–23 | 从 0 开始的 u32 小端序号 |
| 24 起 | PCM16 小端采样 |

控制和音频共享有序 WebSocket。当前会话重复音频不输出两次，序号缺口或不匹配的末帧声明中止会话；旧会话音频丢弃。短按在 Priming 阶段直接取消，不回放。

状态机为 Disconnected → Idle → Priming → Streaming → Draining → Idle。主线程处理原生托盘事件；Tokio actor 串行拥有会话和输入代理；音频 worker 负责重采样，CPAL 回调只读取有界环形缓冲并输出静音或采样。

输入代理通过继承的私有 stdin/stdout 管道接收 `press / renew / release / tap_enter / exit`。它持有实际按键，租约 1 秒；管道 EOF、租约超时均释放。租约失效后不能通过 renew 重新按键。主进程检测代理退出或请求失败后补发紧急释放并暂停。

本机管理 API 只绑定回环地址，要求 Bearer 管理令牌，验证 Host 和存在时的 Origin；不提供跨域允许头。手机服务不挂载管理路由。配置修改由同一 actor 在空闲时执行，持久化失败回退启动项，不与新会话并发提交。

启动等待初值 350ms，配置范围 100–450ms；启动超时 500ms；音频排队上限 750ms；尾部静音 150ms；正常结束期限 800ms；心跳 250ms，失联 1 秒中止。所有超时均以单调时钟计算。

## 松手 Enter 扩展

协议版本仍为 1。Windows 的服务端 `hello` 增加 `capabilities:["release_enter"]`；手机仅在声明该能力时发送 `end.action`，未声明时省略字段并维持原有结束操作。`action` 仅接受 `finish` 或 `enter`，缺省 `finish`；只在当前会话首次结束时采用，重复结束不修改动作。帧协议不变。

`ended` 增加 `enter_pending`，仍只确认音频排空和语音键释放；随后 `action_result(session_id,status,reason)` 报告 `executed` / `skipped` / `failed`。成功只表示 Enter 按下／释放已被系统接受，不确认识别或发送结果。旧消息缺少能力和 pending 字段时按空／false 处理。

控制器保存与连接、会话绑定的单次待发动作；释放成功后按 `enter_delay_ms`（默认 1000，范围 0–5000）非阻塞等待，并继续收心跳和控制消息。新会话、取消、失联、暂停、配置变更和退出均清除动作。仅正常完成的会话可以回车，错误不重试。代理保存开始说话时的前台窗口、线程／进程和原生焦点控件，回车前再核对；不抢焦点、不读取输入正文。Windows Enter 使用 SendInput，与 UMDF 语音修饰键驱动无关。

音频工作通道采用 40 个槽位的有界异步队列，发送最长等待 100ms；750ms 积压预算同时计算待处理输入与输出环形缓冲，槽位数不允许绕过时间上限。清空、关闭及尾部刷新须取得工作线程确认，清空与语音键释放并行；结束期限包含尾部刷新的等待。发送背压仅在控制器异步任务中等待，设备音频回调保持无阻塞。过载中止保留明确操作错误，不发送 Enter。
