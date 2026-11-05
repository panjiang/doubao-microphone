# 原生 iOS 接入（桌面 0.2.2 / App 0.1.0 构建 23）

现有网页 `/pair`、`/pair-result/{id}`、`/ws` 同样取消二次确认，保持严格 Origin 校验。管理端仍只绑定回环地址、校验管理令牌；新增原生接口仅在手机 HTTPS 服务中暴露。

## 二维码

POST `/api/pair-ticket` 返回一个统一链接和一个二维码，不再区分网页码和 App 码。`qr_svg` 与兼容字段 `native_qr_svg` 内容相同。五分钟一次性令牌、随机 `ticket_id`、成功后自动收起及旧响应过滤保持原逻辑；任一客户端提交即消费令牌。

统一格式如下（示意内容，不是可用凭据）：

```text
http://局域网IPv4:向导端口/setup?token=一次性令牌#v=1&port=HTTPS端口&ca=Base64URL证书
```

`ca` 是本机保存的 CA 公钥证书 DER，使用无 padding 的 Base64URL；不含私钥、管理令牌或已有设备凭据。二维码不写日志。

App 在本地解析整个链接，校验协议、IPv4、端口、单一 ticket、版本、唯一 fragment 字段及 DER 证书；随后直接通过专用信任锚访问 HTTPS，不向 HTTP 地址请求证书，不接受任意证书。浏览器忽略未发送到服务器的 fragment，沿用原 HTTP 首次向导和 Safari 手动安装、信任证书的流程。因此相机与 App 能扫描同一个码，原有信任边界不变。

App 构建 23 继续识别旧版 JSON：`type=doubao_ptt_pair`、`version=1`、`host`、`port`、`ticket`、`ca_der`。旧 App 构建 22 及之前只识别 JSON，使用新统一码前需更新 App；已保存配对凭据与音频协议无需迁移。

## 入口

- POST `/native/pair`：沿用 `{ticket,label}`，返回 `{id,poll}`，有效二维码自动完成配对，无需管理页确认。
- GET `/native/pair-result/{id}?poll=…`：沿用一次性设备令牌领取逻辑。
- WSS `/native/ws`：五秒内发协议 v1 hello，通过设备令牌验证后才连接音频与会话控制器；仍只允许一个手机控制。

原生入口校验 Host／HTTP2 authority。Origin 存在时必须等于当前 HTTPS 地址；不存在时仍需上述配对、轮询或设备凭据，不把“没有 Origin”视为认证。

hello、start、end、cancel、音频头及 release_enter 能力协商未改变。收到应用层 heartbeat 后现在回复 heartbeat，原网页忽略该消息，原生 App 用它判定一秒失联。PCM 和控制消息共用串行发送链，end 位于尾帧之后。

Keychain 仅保存一台电脑，新配对成功后替换手机记录。桌面仍保留独立设备凭据和撤销操作；替换手机记录不等于删除电脑上的旧设备。

编译成功不代表原生 TLS、网页兼容或真机语音链路已实测，均需分别记录用户验收结果。
