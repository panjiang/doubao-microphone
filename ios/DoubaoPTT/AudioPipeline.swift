import AVFoundation
import Foundation

private final class AudioRingOwner: @unchecked Sendable {
    let pointer:OpaquePointer
    init(capacity:UInt32) throws {
        guard let pointer=ptt_ring_create(capacity) else { throw PTTError.message("无法准备音频缓冲。") }
        self.pointer=pointer
    }
    deinit { ptt_ring_destroy(pointer) }
}

final class AudioPipeline: @unchecked Sendable {
    var onLevel:((Double)->Void)?
    var onFailure:((String)->Void)?
    private let work=DispatchQueue(label:"ptt.audio.convert",qos:.userInitiated)
    private var engine:AVAudioEngine?
    private var ring:AudioRingOwner?
    private var timer:DispatchSourceTimer?
    private var source:AVAudioPCMBuffer?
    private var output:AVAudioPCMBuffer?
    private var converter:AVAudioConverter?
    private var rate:Double=48000
    private var sessionID:UUID?
    private var sequence:UInt32=0
    private var samples:[Int16]=[]
    private var frame:((Data,Int)->Void)?
    private var failed=false
    private var lastMeter:TimeInterval=0
    private var savedRoute=""
    private var epoch=0
    var running:Bool { engine != nil }

    private func route() -> String {
        let session=AVAudioSession.sharedInstance()
        return session.currentRoute.inputs.map(\.uid).joined(separator:"|")+"/\(session.sampleRate)/\(session.inputNumberOfChannels)"
    }
    var routeChanged:Bool { running && route() != savedRoute }

    func start() throws {
        precondition(Thread.isMainThread)
        if running { return }
        let audio=AVAudioSession.sharedInstance()
        try audio.setCategory(.record,mode:.measurement,options:[])
        try audio.setPreferredIOBufferDuration(0.02)
        try audio.setActive(true)
        if let builtIn=audio.availableInputs?.first(where:{$0.portType == .builtInMic}) { try audio.setPreferredInput(builtIn) }
        let engine=AVAudioEngine();let node=engine.inputNode;let format=node.outputFormat(forBus:0)
        guard format.sampleRate>=8000,format.sampleRate<=192000,format.channelCount>0,format.channelCount<=8,
              format.commonFormat == .pcmFormatFloat32,!format.isInterleaved,
              let mono=AVAudioFormat(standardFormatWithSampleRate:format.sampleRate,channels:1),
              let wire=AVAudioFormat(standardFormatWithSampleRate:24000,channels:1),
              let input=AVAudioPCMBuffer(pcmFormat:mono,frameCapacity:4096),
              let output=AVAudioPCMBuffer(pcmFormat:wire,frameCapacity:16384),
              let converter=AVAudioConverter(from:mono,to:wire) else {
            try? audio.setActive(false);throw PTTError.message("手机当前音频格式不可用，请断开外接音频设备后重试。")
        }
        let ring=try AudioRingOwner(capacity:UInt32(format.sampleRate/2))
        work.sync {
            self.epoch+=1
            self.ring=ring;self.source=input;self.output=output;self.converter=converter
            self.rate=format.sampleRate;self.failed=false;self.sessionID=nil;self.samples.removeAll(keepingCapacity:true)
        }
        node.installTap(onBus:0,bufferSize:1024,format:format) { buffer,_ in
            // No allocation, conversion, dispatch, disk or network operation in this callback.
            guard let channels=buffer.floatChannelData else { return }
            let pointers=UnsafeRawPointer(channels).assumingMemoryBound(to:UnsafePointer<Float>?.self)
            ptt_ring_write(ring.pointer,pointers,buffer.format.channelCount,buffer.frameLength)
        }
        do { engine.prepare();try engine.start() }
        catch { node.removeTap(onBus:0);work.sync { self.ring=nil };try? audio.setActive(false);throw error }
        self.engine=engine;savedRoute=route()
        work.sync {
            let timer=DispatchSource.makeTimerSource(queue:work);timer.schedule(deadline:.now(),repeating:0.01)
            timer.setEventHandler { [weak self] in self?.consume() };self.timer=timer;timer.resume()
        }
    }
    func begin(_ id:UUID,onFrame:@escaping(Data,Int)->Void) {
        work.async { [weak self] in
            guard let self,!failed,let ring else { return }
            ptt_ring_clear(ring.pointer);converter?.reset();samples.removeAll(keepingCapacity:true)
            sequence=0;sessionID=id;frame=onFrame
        }
    }
    func finish(_ id:UUID,completion:@escaping(UInt32?)->Void) {
        work.async { [weak self] in
            guard let self,sessionID==id,!failed else { return }
            consume()
            guard !failed else { return }
            do {
                try convert(ending:true)
                flush()
                let last:UInt32?=sequence==0 ? nil:sequence-1
                sessionID=nil;frame=nil;samples.removeAll(keepingCapacity:true);converter?.reset()
                completion(last)
            } catch { fail("音频尾部处理失败，本次输入已取消。") }
        }
    }
    func cancel(completion:(()->Void)?=nil) {
        work.async { [weak self] in
            guard let self else { completion?();return };sessionID=nil;frame=nil;samples.removeAll(keepingCapacity:true)
            converter?.reset();if let ring { ptt_ring_clear(ring.pointer) }
            completion?()
        }
    }
    func stop() {
        precondition(Thread.isMainThread)
        engine?.inputNode.removeTap(onBus:0);engine?.stop();engine=nil
        work.sync { epoch+=1;timer?.cancel();timer=nil;sessionID=nil;frame=nil;samples.removeAll();ring=nil;source=nil;output=nil;converter=nil;failed=true }
        try? AVAudioSession.sharedInstance().setActive(false,options:.notifyOthersOnDeactivation)
    }
    private func consume() {
        guard !failed,let ring,let source,let pointer=source.floatChannelData?[0] else { return }
        if ptt_ring_failed(ring.pointer) != 0 { fail("音频缓冲积压，本次输入已停止。");return }
        if sessionID != nil && Double(ptt_ring_count(ring.pointer))/rate>0.2 { fail("采音处理延迟超过 200ms，本次输入已停止。");return }
        while !failed {
            let count=ptt_ring_read(ring.pointer,pointer,source.frameCapacity)
            if count==0 { break }
            source.frameLength=count
            let now=ProcessInfo.processInfo.systemUptime
            if now-lastMeter>0.08 {
                var energy:Double=0;for i in 0..<Int(count) { energy+=Double(pointer[i]*pointer[i]) }
                let level=min(1,sqrt(energy/Double(count))*5);lastMeter=now
                let current=epoch
                DispatchQueue.main.async { [weak self] in guard let self,epoch==current else { return };onLevel?(level) }
            }
            if sessionID != nil {
                do { try convert(ending:false) } catch { fail("音频转换失败，本次输入已停止。") }
            }
        }
    }
    private func convert(ending:Bool) throws {
        guard let converter,let source,let output else { throw PTTError.message("音频尚未就绪。") }
        var supplied=false
        for _ in 0..<8 {
            var error:NSError?
            output.frameLength=0
            let status=converter.convert(to:output,error:&error) { _,state in
                if !ending && !supplied { supplied=true;state.pointee = .haveData;return source }
                state.pointee=ending ? .endOfStream:.noDataNow;return nil
            }
            if let error { throw error }
            if status == .error { throw PTTError.message("音频转换失败。") }
            if let data=output.floatChannelData?[0] {
                for i in 0..<Int(output.frameLength) {
                    let value=data[i].isFinite ? max(-1,min(1,data[i])):0
                    samples.append(Int16(max(-32768,min(32767,Int(value*32768)))))
                    if samples.count==480 { flush() }
                }
            }
            if !ending || status == .endOfStream || status == .inputRanDry { return }
        }
        throw PTTError.message("音频尾部转换超时。")
    }
    private func flush() {
        guard let id=sessionID,!samples.isEmpty else { return }
        var packet=Data("PPT1".utf8);var uuid=id.uuid;withUnsafeBytes(of:&uuid) { packet.append(contentsOf:$0) }
        var seq=sequence.littleEndian;withUnsafeBytes(of:&seq) { packet.append(contentsOf:$0) }
        for sample in samples { let bits=UInt16(bitPattern:sample);packet.append(UInt8(truncatingIfNeeded:bits));packet.append(UInt8(truncatingIfNeeded:bits>>8)) }
        frame?(packet,samples.count);sequence &+= 1;samples.removeAll(keepingCapacity:true)
    }
    private func fail(_ reason:String) {
        guard !failed else { return };failed=true;sessionID=nil;samples.removeAll();frame=nil
        let current=epoch
        DispatchQueue.main.async { [weak self] in guard let self,epoch==current else { return };onFailure?(reason) }
    }
}
