import Foundation

final class SocketClient: @unchecked Sendable {
    var onMessage:(([String:Any])->Void)?
    var onFailure:((String)->Void)?
    private let queue=DispatchQueue(label:"ptt.socket")
    private let session:URLSession
    private var socket:URLSessionWebSocketTask?
    private struct Packet { let value:URLSessionWebSocketTask.Message;let samples:Int;let created:TimeInterval }
    private var packets:[Packet]=[]
    private var pendingSamples=0
    private var sending=false
    private var closed=false
    private var hello:CheckedContinuation<Void,Error>?
    private var timer:DispatchSourceTimer?
    private var lastReceived=ProcessInfo.processInfo.systemUptime
    private var opened=ProcessInfo.processInfo.systemUptime
    private var ready=false
    private var sendGeneration=0
    private let budgetLock=NSLock()
    private var budgetSamples=0
    private var budgetOpen=true
    private var overloadQueued=false
    let computer:Computer

    init(_ computer:Computer) { self.computer=computer;session=pinnedSession(computer) }
    func connect() async throws {
        try await withCheckedThrowingContinuation { (continuation:CheckedContinuation<Void,Error>) in
            queue.async { [self] in
                guard !closed else { continuation.resume(throwing:PTTError.message("连接已取消。"));return }
                hello=continuation;opened=ProcessInfo.processInfo.systemUptime;lastReceived=opened
                socket=session.webSocketTask(with:computer.url("/native/ws",websocket:true));socket?.maximumMessageSize=65536;socket?.resume()
                enqueueJSON(["type":"hello","version":1,"token":computer.token,"sample_rate":24000,"channels":1])
                receive()
                let timer=DispatchSource.makeTimerSource(queue:queue);timer.schedule(deadline:.now()+0.25,repeating:0.25)
                timer.setEventHandler { [weak self] in self?.tick() };self.timer=timer;timer.resume()
            }
        }
    }
    func control(_ value:[String:Any]) { queue.async { [weak self] in self?.enqueueJSON(value) } }
    func audio(_ data:Data,samples:Int) {
        let created=ProcessInfo.processInfo.systemUptime
        budgetLock.lock()
        guard budgetOpen else { budgetLock.unlock();return }
        if budgetSamples+samples>4800 {
            let notify = !overloadQueued;overloadQueued=true;budgetLock.unlock()
            if notify { queue.async { [weak self] in self?.fail("发送积压超过 200ms，本次输入已停止。") } }
            return
        }
        budgetSamples+=samples;budgetLock.unlock()
        queue.async { [weak self] in
            guard let self else { return }
            guard !closed else { releaseBudget(samples);return }
            guard pendingSamples+samples<=4800 else { fail("发送积压超过 200ms，本次输入已停止。");return }
            pendingSamples+=samples;packets.append(Packet(value:.data(data),samples:samples,created:created));sendNext()
        }
    }
    private func releaseBudget(_ samples:Int) { budgetLock.lock();budgetSamples=max(0,budgetSamples-samples);budgetLock.unlock() }
    private func enqueueJSON(_ value:[String:Any]) {
        guard !closed else { return }
        guard packets.count<64,let data=try? JSONSerialization.data(withJSONObject:value),let text=String(data:data,encoding:.utf8) else { fail("发送队列异常，本次输入已停止。");return }
        packets.append(Packet(value:.string(text),samples:0,created:ProcessInfo.processInfo.systemUptime));sendNext()
    }
    private func sendNext() {
        guard !closed,!sending,!packets.isEmpty,let socket else { return }
        let packet=packets[0]
        if packet.samples>0 && ProcessInfo.processInfo.systemUptime-packet.created>0.2 { fail("发送延迟超过 200ms，本次输入已停止。");return }
        sending=true;sendGeneration+=1;let generation=sendGeneration
        socket.send(packet.value) { [weak self] error in
            self?.queue.async { [weak self] in
                guard let self,!closed,generation==sendGeneration else { return }
                if error != nil { fail("网络发送失败，请重新按住连接。");return }
                pendingSamples-=packet.samples;releaseBudget(packet.samples);packets.removeFirst();sending=false;sendNext()
            }
        }
        // Initial hello includes TCP/TLS/WebSocket setup; the 200ms bound applies after connection.
        let sendTimeout:Double=ready ? 0.2:5
        queue.asyncAfter(deadline:.now()+sendTimeout) { [weak self] in
            guard let self,!closed,sending,sendGeneration==generation else { return }
            fail(ready ? "网络发送超时，本次输入已停止，请松手后重新按住。":"连接电脑超时，请检查电脑程序和局域网，再重新按住。")
        }
    }
    private func receive() {
        socket?.receive { [weak self] result in
            self?.queue.async { [weak self] in
                guard let self,!closed else { return }
                switch result {
                case .failure: fail("电脑连接已断开，请检查网络或重新扫码配对。")
                case .success(let message):
                    guard case .string(let text)=message,let data=text.data(using:.utf8),let value=try? JSONSerialization.jsonObject(with:data) as? [String:Any] else { fail("电脑协议响应无法识别。");return }
                    lastReceived=ProcessInfo.processInfo.systemUptime
                    if value["type"] as? String=="error" { fail(value["message"] as? String ?? "电脑拒绝本次输入。");return }
                    if value["type"] as? String=="hello" {
                        guard value["version"] as? Int==1 else { fail("电脑协议版本不支持，请更新电脑程序。");return }
                        ready=true;hello?.resume();hello=nil
                    }
                    DispatchQueue.main.async { [weak self] in self?.onMessage?(value) }
                    receive()
                }
            }
        }
    }
    private func tick() {
        let now=ProcessInfo.processInfo.systemUptime
        if !ready && now-opened>5 { fail("连接电脑超时，请检查电脑程序和局域网，再重新按住。");return }
        if ready && now-lastReceived>1 { fail("电脑响应超时，本次输入已停止，请松手后重新按住。");return }
        if ready { enqueueJSON(["type":"heartbeat"]) }
    }
    private func fail(_ reason:String) {
        guard !closed else { return };shutdown(reason)
        DispatchQueue.main.async { [weak self] in self?.onFailure?(reason) }
    }
    func close() { queue.async { [weak self] in self?.shutdown() } }
    private func shutdown(_ reason:String="电脑连接已取消，请重新按住连接。") {
        guard !closed else { return };closed=true
        budgetLock.lock();budgetOpen=false;budgetSamples=0;budgetLock.unlock()
        timer?.cancel();timer=nil;packets.removeAll();pendingSamples=0;sending=false
        hello?.resume(throwing:PTTError.message(reason));hello=nil
        socket?.cancel(with:.goingAway,reason:nil);socket=nil;session.invalidateAndCancel()
    }
    deinit { timer?.cancel();socket?.cancel(with:.goingAway,reason:nil);session.invalidateAndCancel() }
}
