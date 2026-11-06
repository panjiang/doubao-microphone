import SwiftUI
import AVFoundation

@MainActor final class AppModel:ObservableObject {
    enum Phase { case idle,connecting,priming,streaming,draining }
    @Published var computer:Computer?
    @Published var status="扫描电脑配置页的配对二维码。"
    @Published var phase:Phase = .idle
    @Published var level:Double=0
    @Published var scanner=false
    @Published var pairing=false
    @Published var connected=false
    private let audio=AudioPipeline()
    private var client:SocketClient?
    private var connectionTask:Task<Void,Never>?
    private var pairTask:Task<Void,Never>?
    private var holdTask:Task<Void,Never>?
    private var holdQualified=false
    private var holdSequence=0
    private var pairClient:PairingClient?
    private var observers:[NSObjectProtocol]=[]
    @Published private(set) var holding=false
    private var foreground=false
    @Published private(set) var generation=0
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
    func scene(_ state:ScenePhase) {
        foreground=state == .active
        // Foreground screen policy is independent of microphone/connection lifetime.
        UIApplication.shared.isIdleTimerDisabled=foreground
        if !foreground { stop("本次输入已结束，返回后请重新按住。") }
        if state == .background { pairTask?.cancel();pairTask=nil;pairClient?.close();pairClient=nil;pairing=false }
    }
    func openScanner() { stop("请扫描电脑配置页的配对二维码。");scanner=true }
    func scan(_ text:String) {
        scanner=false
        guard text.utf8.count<=16384 else { status="二维码数据过大或无效。";return }
        do {
            let payload=try PairPayload.parse(text)
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
                    if !Task.isCancelled { self?.status=(error as? RemoteError)?.localizedDescription ?? "配对失败，请允许局域网访问并检查电脑地址、证书和网络。" }
                }
                pairingClient.close()
                if !Task.isCancelled { self?.pairing=false;self?.pairClient=nil;self?.pairTask=nil }
            }
        } catch { status=(error as? RemoteError)?.localizedDescription ?? "请扫描电脑配置页中的配对二维码。" }
    }
    func press() {
        guard computer != nil,!pairing,foreground,!holding else { return }
        holding=true;holdQualified=false;holdSequence+=1
        let sequence=holdSequence,attempt=generation
        if let pending=pendingEnter { client?.control(["type":"cancel","session_id":pending.uuidString]);pendingEnter=nil }
        if phase == .draining { cancelEnterAfterDrain=true }
        holdTask?.cancel()
        holdTask=Task { [weak self] in
            do { try await Task.sleep(nanoseconds:150_000_000) } catch { return }
            guard let self,!Task.isCancelled,holding,foreground,holdSequence==sequence,generation==attempt else { return }
            holdTask=nil;holdQualified=true;prepareHeldSpeech()
        }
    }
    private func prepareHeldSpeech() {
        guard holding,holdQualified,foreground else { return }
        if phase == .draining { cancelEnterAfterDrain=true;status="正在结束上一轮，请保持按住。";return }
        if connected && audio.running { begin();return }
        guard connectionTask==nil else { return }
        let attempt=generation;phase = .connecting;status=connected ? "正在准备麦克风。":"正在准备麦克风和电脑连接。"
        connectionTask=Task { [weak self] in
            guard let self else { return }
            do {
                let permitted=await AVAudioApplication.requestRecordPermission()
                try Task.checkCancellation()
                guard permitted else { throw RemoteError.message("请在 iPhone 设置中允许本 App 使用麦克风。") }
                guard holding,foreground,attempt==generation else { return }
                try audio.start()
                if !connected {
                    guard let computer else { throw RemoteError.message("请重新扫码配对电脑。") }
                    let next=SocketClient(computer);client=next
                    next.onMessage={ [weak self,weak next] event in
                        guard let self,let next,self.client === next else { return };receive(event)
                    }
                    next.onFailure={ [weak self,weak next] reason in
                        guard let self,let next,self.client === next else { return };stop(reason)
                    }
                    try await next.connect();try Task.checkCancellation()
                }
                guard attempt==generation,foreground,holding else { return }
                connected=true;connectionTask=nil;begin()
            } catch {
                if attempt==generation && !Task.isCancelled {
                    stop((error as? RemoteError)?.localizedDescription ?? "连接失败，请检查权限、电脑程序和局域网。")
                }
            }
            if attempt==generation { connectionTask=nil }
        }
    }
    private func begin() {
        guard holding,holdQualified,foreground,connected,audio.running,sessionID==nil,phase != .draining,let client else { return }
        let id=UUID();sessionID=id;pendingEnter=nil;phase = .priming;status="准备中，请开始说话。"
        client.control(["type":"start","session_id":id.uuidString])
        audio.begin(id) { [weak client] data,count in client?.audio(data,samples:count) }
    }
    func release(finishOnly:Bool) {
        let qualified=holdQualified
        holding=false;holdQualified=false;holdSequence+=1;holdTask?.cancel();holdTask=nil
        if phase == .draining { return }
        guard let id=sessionID else {
            if phase == .connecting { stop("连接准备已取消，请重新按住。") }
            else if !qualified { status="按住说话即可开始。" }
            return
        }
        guard let client else { stop("电脑连接已断开。");return }
        let attempt=generation
        if phase == .priming {
            phase = .draining;status="正在取消本次短按。"
            // Queue cancel behind every already-produced frame, as with a normal end.
            audio.cancel { [weak self,weak client] in
                Task { @MainActor in
                    guard let self,let client else { return }
                    self.finishAudio(id,attempt:attempt,client:client,message:["type":"cancel","session_id":id.uuidString])
                }
            }
            waitForEnd(id);return
        }
        guard phase == .streaming else { stop("本次输入已取消，请重新按住。");return }
        phase = .draining;status="正在完成输入。"
        let action=releaseEnter && !finishOnly ? "enter":"finish"
        audio.finish(id) { [weak self,weak client] last in
            Task { @MainActor in
                guard let self,let client else { return }
                self.finishAudio(id,attempt:attempt,client:client,message:["type":"end","session_id":id.uuidString,"last_seq":last.map { $0 as Any } ?? NSNull(),"action":action])
            }
        }
        waitForEnd(id)
    }
    private func finishAudio(_ id:UUID,attempt:Int,client:SocketClient,message:[String:Any]) {
        // All tail frames are already queued on the socket. A stale completion may
        // neither send controls nor stop the microphone of a newer session.
        guard generation==attempt,sessionID==id,phase == .draining,self.client === client else { return }
        client.control(message)
        stopMicrophone()
    }
    private func stopMicrophone() {
        audio.stop();level=0
    }
    private func waitForEnd(_ id:UUID) {
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
        if type=="ready",id==sessionID,phase == .priming { phase = .streaming;status="正在向电脑输入声音。" }
        if type=="ended",id==sessionID {
            let resumeHeld=cancelEnterAfterDrain
            // Covers unsolicited aborts as well as an end racing its audio completion.
            stopMicrophone()
            endTimer?.cancel();endTimer=nil;sessionID=nil;phase = .idle
            let complete=event["complete"] as? Bool ?? false
            pendingEnter=complete && (event["enter_pending"] as? Bool ?? false) ? id:nil
            if cancelEnterAfterDrain {
                if pendingEnter != nil { client?.control(["type":"cancel","session_id":id.uuidString]);pendingEnter=nil }
                cancelEnterAfterDrain=false
            }
            status=complete ? (pendingEnter == nil ? "本次语音已结束，请在电脑确认输入结果。":"语音已结束，等待电脑发送回车。") : "本次输入已取消或不完整，请重新按住。"
            if holding && resumeHeld {
                if holdQualified { prepareHeldSpeech() } else { status="请保持按住，准备输入。" }
            } else if holding {
                // An unsolicited abort must never restart the same physical hold.
                holding=false;holdQualified=false;holdSequence+=1;holdTask?.cancel();holdTask=nil
            }
        }
        if type=="action_result",id==pendingEnter,sessionID==nil {
            pendingEnter=nil
            switch event["status"] as? String { case "executed":status="电脑已发送回车。";case "skipped":status="已跳过回车，请在电脑确认输入结果。";default:status="回车未能完成，请在电脑确认。" }
        }
    }
    func stop(_ reason:String) {
        generation+=1;holding=false;holdQualified=false;holdSequence+=1;holdTask?.cancel();holdTask=nil;connectionTask?.cancel();connectionTask=nil;endTimer?.cancel();endTimer=nil
        if let id=sessionID { client?.control(["type":"cancel","session_id":id.uuidString]) }
        if let id=pendingEnter { client?.control(["type":"cancel","session_id":id.uuidString]) }
        stopMicrophone();client?.close();client=nil
        sessionID=nil;pendingEnter=nil;connected=false;releaseEnter=false;cancelEnterAfterDrain=false;phase = .idle;level=0;status=reason
    }
}
