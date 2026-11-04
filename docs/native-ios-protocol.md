# 原生 iOS 接入（桌面 0.2.0）

现有网页 `/pair`、`/pair-result/{id}`、`/ws` 同样取消二次确认，保持严格 Origin 校验。管理端仍只绑定回环地址、校验管理令牌；新增原生接口仅在手机 HTTPS 服务中暴露。

## 二维码

POST /api/pair-ticket 额外返回 `native_qr_svg`，与原 `qr_svg` 使用同一个五分钟一次性令牌。配置页切换网页／App 仅切换显示，生成新二维码仍使旧令牌失效，任一客户端提交后消费该令牌。另返回随机 `ticket_id` 用于配置页关联二维码，不包含认证能力；手机通过 poll 领取凭据后，管理端 status 返回 `paired_ticket`。配置页仅在其匹配当前显示的 `ticket_id` 时清除二维码及链接，生成新二维码或切换连接方式时忽略旧响应。

App 二维码为 JSON：`type=doubao_ptt_pair`、`version=1`、`host`、`port`、`ticket`、`ca_der`。CA 为保存的公钥证书 DER 的标准 Base64，无私钥、管理令牌或已有设备凭据。二维码不写日志。

## 入口

- POST `/native/pair`：沿用 `{ticket,label}`，返回 `{id,poll}`，有效二维码自动完成配对，无需管理页确认。
- GET `/native/pair-result/{id}?poll=…`：沿用一次性设备令牌领取逻辑。
- WSS `/native/ws`：五秒内发协议 v1 hello，通过设备令牌验证后才连接音频与会话控制器；仍只允许一个手机控制。

原生入口校验 Host／HTTP2 authority。Origin 存在时必须等于当前 HTTPS 地址；不存在时仍需上述配对、轮询或设备凭据，不把“没有 Origin”视为认证。

hello、start、end、cancel、音频头及 release_enter 能力协商未改变。收到应用层 heartbeat 后现在回复 heartbeat，原网页忽略该消息，原生 App 用它判定一秒失联。PCM 和控制消息共用串行发送链，end 位于尾帧之后。

Keychain 仅保存一台电脑，新配对成功后替换手机记录。桌面仍保留独立设备凭据和撤销操作；替换手机记录不等于删除电脑上的旧设备。

编译成功不代表原生 TLS、网页兼容或真机语音链路已实测，均需分别记录用户验收结果。
