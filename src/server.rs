use crate::{
    config::Settings,
    controller::Controller,
    identity::Identities,
    protocol::{self, ClientMessage, ServerMessage},
};
use anyhow::{Result, ensure};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

static WEB: include_dir::Dir = include_dir::include_dir!("$CARGO_MANIFEST_DIR/web/dist");

pub struct App {
    pub controller: Controller,
    pub identities: Mutex<Identities>,
    pub dir: PathBuf,
    pub admin_token: String,
    pub initial: Settings,
    pub profile: String,
    pub mock: bool,
}
type Shared = Arc<App>;
pub struct ApiError(anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":self.0.to_string()})),
        )
            .into_response()
    }
}
type Api<T> = std::result::Result<T, ApiError>;
fn check(condition: bool, message: &str) -> Result<()> {
    ensure!(condition, "{message}");
    Ok(())
}

fn asset(path: &str) -> Response {
    if let Some(f) = WEB.get_file(path) {
        let mime = if path.ends_with(".html") {
            "text/html; charset=utf-8"
        } else if path.ends_with(".js") {
            "text/javascript; charset=utf-8"
        } else if path.ends_with(".css") {
            "text/css; charset=utf-8"
        } else {
            "application/octet-stream"
        };
        (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (header::REFERRER_POLICY, "no-referrer"),
            ],
            f.contents(),
        )
            .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
async fn assets(Path(p): Path<String>) -> Response {
    asset(&format!("assets/{p}"))
}
async fn admin_page() -> Response {
    asset("admin.html")
}
async fn phone_page() -> Response {
    asset("phone.html")
}
async fn worklet() -> Response {
    asset("audio-worklet.js")
}

fn authorize(app: &App, h: &HeaderMap) -> Result<()> {
    let wanted = format!("127.0.0.1:{}", app.initial.admin_port);
    ensure!(
        h.get(header::HOST).and_then(|v| v.to_str().ok()) == Some(wanted.as_str()),
        "invalid management host"
    );
    if let Some(origin) = h.get(header::ORIGIN) {
        ensure!(
            origin.to_str()? == format!("http://{wanted}"),
            "invalid management origin"
        );
    }
    let token = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    ensure!(
        crate::identity::matches(&crate::identity::digest(&app.admin_token), token),
        "management authentication required"
    );
    Ok(())
}
fn request_authority<'a>(h: &'a HeaderMap, uri: &'a Uri) -> Option<&'a str> {
    h.get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .or_else(|| uri.authority().map(|a| a.as_str()))
}
fn mobile_origin(app: &App, h: &HeaderMap, uri: &Uri) -> Result<()> {
    let host = format!("{}:{}", app.initial.lan_ip, app.initial.mobile_port);
    ensure!(
        request_authority(h, uri) == Some(host.as_str()),
        "invalid phone host"
    );
    ensure!(
        h.get(header::ORIGIN).and_then(|v| v.to_str().ok())
            == Some(format!("https://{host}").as_str()),
        "invalid phone origin"
    );
    Ok(())
}

pub fn admin_router(app: Shared) -> Router {
    Router::new()
        .route("/", get(admin_page))
        .route("/assets/{*path}", get(assets))
        .route("/api/status", get(status))
        .route("/api/config", get(read_config).put(write_config))
        .route("/api/setup", get(setup))
        .route("/api/pair-ticket", post(ticket))
        .route("/api/pair-approve/{id}", post(approve))
        .route("/api/devices", get(list_devices))
        .route("/api/devices/{id}", delete(revoke))
        .route("/api/pause", put(pause))
        .route("/api/test-key", post(test_key))
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .with_state(app)
}
pub fn mobile_router(app: Shared) -> Router {
    Router::new()
        .route("/", get(phone_page))
        .route("/assets/{*path}", get(assets))
        .route("/audio-worklet.js", get(worklet))
        .route("/info", get(info))
        .route("/pair", post(pair))
        .route("/pair-result/{id}", get(pair_result))
        .route("/ws", get(upgrade))
        .layer(axum::extract::DefaultBodyLimit::max(4096))
        .with_state(app)
}
pub fn bootstrap_router(app: Shared) -> Router {
    Router::new()
        .route("/setup", get(bootstrap))
        .route("/ca.mobileconfig", get(profile))
        .with_state(app)
}

async fn status(State(a): State<Shared>, h: HeaderMap) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    Ok(Json(
        json!({"status":a.controller.status.borrow().clone(),"pending":a.identities.lock().await.pending()}),
    ))
}
async fn test_key(State(a): State<Shared>, h: HeaderMap) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    a.controller.test_shortcut().await?;
    Ok(Json(json!({"countdown_ms":3000,"hold_ms":3000})))
}
async fn read_config(State(a): State<Shared>, h: HeaderMap) -> Api<Json<Settings>> {
    authorize(&a, &h)?;
    Ok(Json(a.controller.settings.lock().await.clone()))
}
async fn write_config(
    State(a): State<Shared>,
    h: HeaderMap,
    Json(s): Json<Settings>,
) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    s.validate()?;
    check(
        a.controller.status.borrow().session_id.is_none(),
        "听写中不能修改配置",
    )?;
    check(
        a.mock || crate::platform::supported(s.shortcut),
        "此快捷键不受当前平台支持",
    )?;
    check(!a.mock || !s.autostart, "模拟模式不修改系统登录启动项")?;
    // Network listener identity is immutable during a process lifetime.
    let restart = s.lan_ip != a.initial.lan_ip
        || s.mobile_port != a.initial.mobile_port
        || s.admin_port != a.initial.admin_port
        || s.bootstrap_port != a.initial.bootstrap_port;
    a.controller.configure(s.clone(), a.dir.clone()).await?;
    Ok(Json(
        json!({"saved":true,"restart_required":restart,"reconnect_required":true}),
    ))
}
async fn setup(State(a): State<Shared>, h: HeaderMap) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    let mock = a.mock;
    let outputs = tokio::task::spawn_blocking(move || crate::audio::devices(mock)).await??;
    Ok(Json(
        json!({"platform":crate::platform::setup_status(),"interfaces":crate::config::interfaces(),"outputs":outputs,"ca_sha256":crate::tls::fingerprint(&a.dir)?}),
    ))
}
async fn ticket(State(a): State<Shared>, h: HeaderMap) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    let token = a.identities.lock().await.issue();
    let url = format!(
        "http://{}:{}/setup?token={token}",
        a.initial.lan_ip, a.initial.bootstrap_port
    );
    let svg = qrcode::QrCode::new(url.as_bytes())?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .build();
    let phone = format!(
        "https://{}:{}/#pair={token}",
        a.initial.lan_ip, a.initial.mobile_port
    );
    Ok(Json(
        json!({"url":url,"phone_url":phone,"qr_svg":svg,"expires_in":300}),
    ))
}
async fn approve(
    State(a): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    a.identities.lock().await.approve(id)?;
    Ok(Json(json!({"approved":true})))
}
async fn list_devices(State(a): State<Shared>, h: HeaderMap) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    Ok(Json(json!(a.identities.lock().await.devices())))
}
async fn revoke(
    State(a): State<Shared>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    a.identities.lock().await.revoke(id)?;
    a.controller.revoke(id).await;
    Ok(Json(json!({"revoked":true})))
}
#[derive(Deserialize)]
struct Pause {
    paused: bool,
}
async fn pause(
    State(a): State<Shared>,
    h: HeaderMap,
    Json(p): Json<Pause>,
) -> Api<Json<serde_json::Value>> {
    authorize(&a, &h)?;
    a.controller.pause(p.paused).await;
    Ok(Json(json!({"paused":p.paused})))
}
async fn info(State(a): State<Shared>) -> Json<serde_json::Value> {
    Json(
        json!({"computer_name":a.controller.settings.lock().await.computer_name,"mock":a.mock,"protocol":1}),
    )
}
#[derive(Deserialize)]
struct Pair {
    ticket: String,
    label: String,
}
async fn pair(
    State(a): State<Shared>,
    h: HeaderMap,
    uri: Uri,
    Json(p): Json<Pair>,
) -> Api<Json<serde_json::Value>> {
    mobile_origin(&a, &h, &uri)?;
    let (id, poll) = a.identities.lock().await.request(&p.ticket, p.label)?;
    Ok(Json(json!({"id":id,"poll":poll})))
}
#[derive(Deserialize)]
struct Poll {
    poll: String,
}
async fn pair_result(
    State(a): State<Shared>,
    Path(id): Path<Uuid>,
    Query(p): Query<Poll>,
) -> Api<Json<serde_json::Value>> {
    let token = a.identities.lock().await.poll(id, &p.poll)?;
    Ok(Json(json!({"token":token})))
}
#[derive(Deserialize)]
struct Ticket {
    token: String,
}
async fn bootstrap(State(a): State<Shared>, Query(t): Query<Ticket>) -> Api<Response> {
    check(
        a.identities.lock().await.bootstrap_allowed(&t.token),
        "二维码已失效，请在电脑配置页重新生成",
    )?;
    let phone = format!(
        "https://{}:{}/#pair={}",
        a.initial.lan_ip, a.initial.mobile_port, t.token
    );
    let certificate = format!(
        "http://{}:{}/ca.mobileconfig?token={}",
        a.initial.lan_ip, a.initial.bootstrap_port, t.token
    );
    let fingerprint = crate::tls::fingerprint(&a.dir)?;
    let html = format!(
        r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>连接语音遥控</title>
<style>body{{font:17px system-ui;max-width:34rem;margin:2rem auto;padding:1rem;line-height:1.8}}a{{color:#1947cf;text-decoration:underline;overflow-wrap:anywhere;-webkit-touch-callout:default}}code{{overflow-wrap:anywhere}}textarea{{display:block;box-sizing:border-box;width:100%;padding:.75rem;font:14px monospace;line-height:1.5;border:1px solid #bcc6d8;border-radius:8px;user-select:text;-webkit-user-select:text}}.hint{{font-size:15px;color:#475569}}</style></head>
<body><h1>首次连接这台电脑</h1><p>已安装并信任证书可直接进入输入页。</p><p><a href="{phone}">打开输入页</a></p>
<h2>首次使用</h2><ol><li><a id="download-certificate" href="{certificate}">下载本机证书描述文件</a></li><li>打开 iPhone 设置 → 通用 → VPN 与设备管理，安装“豆包语音遥控 · 本机证书”。</li><li>设置 → 通用 → 关于本机 → 证书信任设置，开启 Doubao PTT Local CA 的完全信任。</li><li>回到此页，打开输入页并请求配对，在电脑配置页确认。</li></ol>
<p class="hint">请用 Safari 下载并安装证书。在 Chrome 中可长按上面的下载链接，复制链接地址后粘贴到 Safari。也可以选择并复制下方完整地址。</p>
<label for="certificate-url">证书下载地址（可选择复制）</label><textarea id="certificate-url" readonly rows="4" spellcheck="false" autocapitalize="off">{certificate}</textarea>
<p>证书 SHA-256（应与电脑配置页一致）：<br><code>{fingerprint}</code></p><p>此描述文件只包含本机根证书，卸载软件后可移除它。二维码和下载链接有效期 5 分钟。</p></body></html>"#
    );
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        html,
    )
        .into_response())
}
async fn profile(State(a): State<Shared>, Query(t): Query<Ticket>) -> Api<Response> {
    check(
        a.identities.lock().await.bootstrap_allowed(&t.token),
        "certificate download expired",
    )?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-apple-aspen-config"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=DoubaoPTT.mobileconfig",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        a.profile.clone(),
    )
        .into_response())
}
async fn upgrade(
    State(a): State<Shared>,
    h: HeaderMap,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> Api<Response> {
    mobile_origin(&a, &h, &uri)?;
    Ok(ws
        .max_message_size(protocol::MAX_MESSAGE)
        .max_frame_size(protocol::MAX_MESSAGE)
        .on_upgrade(move |s| socket(a, s)))
}

async fn socket(a: Shared, mut socket: WebSocket) {
    let hello = tokio::time::timeout(Duration::from_secs(5), socket.recv()).await;
    let Some(Ok(Message::Text(text))) = hello.ok().flatten() else {
        return;
    };
    let Ok(ClientMessage::Hello {
        version,
        token,
        sample_rate,
        channels,
    }) = serde_json::from_str(&text)
    else {
        return;
    };
    if version != protocol::VERSION || protocol::validate_format(sample_rate, channels).is_err() {
        let _ = send_error(&mut socket, "format", "协议或音频格式不支持").await;
        return;
    }
    let Some(device) = a.identities.lock().await.authenticate(&token) else {
        let _ = send_error(&mut socket, "auth", "请重新扫码配对").await;
        return;
    };
    let (event_tx, mut events) = mpsc::channel(16);
    let connection = Uuid::new_v4();
    if let Err(e) = a
        .controller
        .connect(device, connection, sample_rate, event_tx)
        .await
    {
        let _ = send_error(&mut socket, "connect", &e.to_string()).await;
        return;
    }
    let (mut sink, mut stream) = socket.split();
    let hello = ServerMessage::Hello {
        version: 1,
        mock: a.mock,
    };
    if sink
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .is_err()
    {
        a.controller.disconnect(connection).await;
        return;
    }
    loop {
        tokio::select! {
            event=events.recv()=>{
                let Some(event)=event else{break};
                if !matches!(tokio::time::timeout(Duration::from_millis(500),sink.send(Message::Text(serde_json::to_string(&event).unwrap().into()))).await,Ok(Ok(()))){break;}
            }
            message=stream.next()=>{
                let Some(Ok(message))=message else{break};
                let result=match message {
                    Message::Text(text)=>match serde_json::from_str::<ClientMessage>(&text){
                        Ok(ClientMessage::Heartbeat)=>{a.controller.heartbeat(connection).await;Ok(())}
                        Ok(ClientMessage::Start{session_id})=>a.controller.start(connection,session_id).await,
                        Ok(ClientMessage::End{session_id,last_seq})=>a.controller.end(connection,session_id,last_seq).await,
                        Ok(ClientMessage::Cancel{session_id})=>{a.controller.cancel(connection,session_id).await;Ok(())}
                        _=>Err(anyhow::anyhow!("invalid control message"))
                    },
                    Message::Binary(b)=>match protocol::AudioFrame::decode(&b,sample_rate){Ok(f)=>a.controller.frame(connection,f).await,Err(e)=>Err(e)},
                    Message::Close(_)=>break,
                    Message::Ping(p)=>{if sink.send(Message::Pong(p)).await.is_err(){break;}Ok(())}
                    _=>Ok(())
                };
                if let Err(e)=result {
                    let event=ServerMessage::Error{code:"session".into(),message:e.to_string()};
                    let _=tokio::time::timeout(Duration::from_millis(500),sink.send(Message::Text(serde_json::to_string(&event).unwrap().into()))).await;break;
                }
            }
        }
    }
    a.controller.disconnect(connection).await;
}
async fn send_error(socket: &mut WebSocket, code: &str, message: &str) -> Result<()> {
    socket
        .send(Message::Text(
            serde_json::to_string(&ServerMessage::Error {
                code: code.into(),
                message: message.into(),
            })?
            .into(),
        ))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http2_authority_does_not_require_an_http1_host_header() {
        let mut h = HeaderMap::new();
        let uri: Uri = "https://127.0.0.1:38443/pair".parse().unwrap();
        assert_eq!(request_authority(&h, &uri), Some("127.0.0.1:38443"));
        h.insert(header::HOST, "different.example".parse().unwrap());
        assert_eq!(request_authority(&h, &uri), Some("different.example"));
    }
    #[test]
    fn embedded_web_has_no_external_runtime_dependency() {
        assert!(WEB.get_file("phone.html").is_some());
        assert!(WEB.get_file("admin.html").is_some());
        assert!(WEB.get_file("audio-worklet.js").is_some());
    }
}
