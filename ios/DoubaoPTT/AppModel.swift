import SwiftUI
import AVFoundation

@MainActor final class AppModel:ObservableObject {
    enum Phase { case idle,connecting,priming,streaming,draining }
    @Published var computer:Computer?
    @Published var status="扫描电脑配置页的 iPhone App 二维码。"
    @Published var phase:Phase = .idle
    @Published var level:Double=0
    @Published var scanner=false
    @Published var pairing=false
    @Published var connected=false
    private let audio=AudioPipeline()
    private var client:SocketClient?
    private var connectionTask:Task<Void,Never>?
    private var pairTask:Task<Void,Never>?
    private var pairClient:PairingClient?
    private var observers:[NSObjectProtocol]=[]
    private var holding=false
    private var foreground=false
    private var generation=0
    private var sessionID:UUID?
    private var pendingEnter:UUID?
    private var releaseEnter=false
    private var cancelEnterAfterDrain=false
    private var endTimer:Task<Void,Never>?

    init() {
        do { computer=try PairStore.load();if computer != nil { status="按住说话即可连接电脑。" } }
        catch { status="无法读取已保存的配对，请解锁手机或重新扫码。" }
        audio.onLevel={ [weak self] level in self?.level=level }
        audio.onFailure={ [weak self] reason in self?.stop(reason) }
        let center=NotificationCenter.default
        observers.append(center.addObserver(forName:AVAudioSession.interruptionNotification,object:nil,queue:.main) { [weak self] note in
            guard let type=note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,type==AVAudioSession.InterruptionType.began.rawValue else { return }
            Task { @MainActor in self?.stop("录音被系统中断，请重新按住。") }
        })
        observers.append(center.addObserver(forName:AVAudioSession.routeChangeNotification,object:nil,queue:.main) { [weak self] _ in
            Task { @MainActor in if self?.audio.routeChanged==true { self?.stop("音频设备已变化，请重新按住。") } }
        })
        observers.append(center.addObserver(forName:AVAudioSession.mediaServicesWereResetNotification,object:nil,queue:.main) { [weak self] _ in
            Task { @MainActor in self?.stop("音频服务已重置，请重新按住。") }
        })
    }
    var buttonLabel:String {
        switch phase { case .idle:return "按住说话";case .connecting:return "正在连接";case .priming:return "准备中";case .streaming:return "正在输入";case .draining:return "正在结束" }
    }
    func scene(_ state:ScenePhase) {
        foreground=state == .active
        if !foreground { stop("本次输入已结束，返回后请重新按住。") }
        if state == .background { pairTask?.cancel();pairTask=nil;pairClient?.close();pairClient=nil;pairing=false }
    }
    func openScanner() { stop("请扫描电脑配置页的 iPhone App 二维码。");scanner=true }
    func scan(_ text:String) {
        scanner=false
        guard text.utf8.count<=16384,let data=text.data(using:.utf8) else { status="二维码数据过大或无效。";return }
        do {
            let payload=try JSONDecoder().decode(PairPayload.self,from:data)
            let pending=try payload.record()
            pairTask?.cancel();pairClient?.close()
            let pairingClient=PairingClient(pending);pairClient=pairingClient
            pairing=true;status="正在连接电脑并完成配对。"
            pairTask=Task { [weak self] in
                do {
                    let saved=try await pairingClient.pair(ticket:payload.ticket)
                    try Task.checkCancellation();try PairStore.save(saved)
                    guard let self else { return };computer=saved;status="配对成功，按住说话即可连接。"
                } catch {
                    if !Task.isCancelled { self?.status=(error as? PTTError)?.localizedDescription ?? "配对失败，请允许局域网访问并检查电脑地址、证书和网络。" }
                }
                pairingClient.close()
                if !Task.isCancelled { self?.pairing=false;self?.pairClient=nil;self?.pairTask=nil }
            }
        } catch { status=(error as? PTTError)?.localizedDescription ?? "请扫描电脑配置页中的 iPhone App 二维码。" }
    }
    func press() {
        guard computer != nil,!pairing,foreground else { return }
        holding=true
        if let pending=pendingEnter { client?.control(["type":"cancel","session_id":pending.uuidString]);pendingEnter=nil }
        if phase == .draining { cancelEnterAfterDrain=true;return }
        if connected && audio.running { begin();return }
        guard connectionTask==nil else { return }
        let attempt=generation;phase = .connecting;status="正在准备麦克风和电脑连接。"
        connectionTask=Task { [weak self] in
            guard let self else { return }
            do {
                let permitted=await AVAudioApplication.requestRecordPermission()
                try Task.checkCancellation()
                guard permitted else { throw PTTError.message("请在 iPhone 设置中允许本 App 使用麦克风。") }
                guard holding,foreground,attempt==generation else { return }
                try audio.start();UIApplication.shared.isIdleTimerDisabled=true
                guard let computer else { return }
                let next=SocketClient(computer);client=next
                next.onMessage={ [weak self,weak next] event in
                    guard let self,let next,self.client === next else { return };receive(event)
                }
                next.onFailure={ [weak self,weak next] reason in
                    guard let self,let next,self.client === next else { return };stop(reason)
                }
                try await next.connect();try Task.checkCancellation()
                guard attempt==generation,foreground,holding else { return }
                connected=true;connectionTask=nil;begin()
            } catch {
                if attempt==generation && !Task.isCancelled {
                    stop((error as? PTTError)?.localizedDescription ?? "连接失败，请检查权限、电脑程序和局域网。")
                }
            }
            if attempt==generation { connectionTask=nil }
        }
    }
    private func begin() {
        guard holding,foreground,connected,sessionID==nil,phase != .draining,let client else { return }
        let id=UUID();sessionID=id;pendingEnter=nil;phase = .priming;status="准备中，请开始说话。"
        client.control(["type":"start","session_id":id.uuidString])
        audio.begin(id) { [weak client] data,count in client?.audio(data,samples:count) }
    }
    func release(above:Bool) {
        holding=false
        if phase == .draining { return }
        guard let id=sessionID else { if phase == .connecting { stop("连接准备已取消，请重新按住。") };return }
        guard phase == .streaming else { stop("本次按住过短，已取消。请重新按住说话。");return }
        guard let client else { stop("电脑连接已断开。");return }
        phase = .draining;status="正在完成输入。"
        let action=releaseEnter && !above ? "enter":"finish"
        audio.finish(id) { [weak client] last in
            client?.control(["type":"end","session_id":id.uuidString,"last_seq":last.map { $0 as Any } ?? NSNull(),"action":action])
        }
        endTimer?.cancel();endTimer=Task { [weak self] in
            try? await Task.sleep(nanoseconds:1_500_000_000)
            if !Task.isCancelled,self?.sessionID==id { self?.stop("结束确认超时，本次输入已停止。") }
        }
    }
    func cancelTouch() { stop("触摸已取消，本次输入已停止。") }
    private func receive(_ event:[String:Any]) {
        let type=event["type"] as? String
        if type=="hello" { releaseEnter=(event["capabilities"] as? [String] ?? []).contains("release_enter");return }
        guard let raw=event["session_id"] as? String,let id=UUID(uuidString:raw) else { return }
        if type=="ready",id==sessionID { phase = .streaming;status="正在向电脑输入声音。" }
        if type=="ended",id==sessionID {
            endTimer?.cancel();endTimer=nil;sessionID=nil;phase = .idle
            let complete=event["complete"] as? Bool ?? false
            pendingEnter=complete && (event["enter_pending"] as? Bool ?? false) ? id:nil
            if cancelEnterAfterDrain {
                if pendingEnter != nil { client?.control(["type":"cancel","session_id":id.uuidString]);pendingEnter=nil }
                cancelEnterAfterDrain=false
            }
            status=complete ? (pendingEnter == nil ? "本次语音已结束，请在电脑确认输入结果。":"语音已结束，等待电脑发送回车。") : "本次输入已取消或不完整，请重新按住。"
            if holding { begin() }
        }
        if type=="action_result",id==pendingEnter,sessionID==nil {
            pendingEnter=nil
            switch event["status"] as? String { case "executed":status="电脑已发送回车。";case "skipped":status="已跳过回车，请在电脑确认输入结果。";default:status="回车未能完成，请在电脑确认。" }
        }
    }
    func stop(_ reason:String) {
        generation+=1;holding=false;connectionTask?.cancel();connectionTask=nil;endTimer?.cancel();endTimer=nil
        if let id=sessionID { client?.control(["type":"cancel","session_id":id.uuidString]) }
        if let id=pendingEnter { client?.control(["type":"cancel","session_id":id.uuidString]) }
        audio.cancel();audio.stop();client?.close();client=nil
        sessionID=nil;pendingEnter=nil;connected=false;releaseEnter=false;cancelEnterAfterDrain=false;phase = .idle;level=0;status=reason
        UIApplication.shared.isIdleTimerDisabled=false
    }
}
