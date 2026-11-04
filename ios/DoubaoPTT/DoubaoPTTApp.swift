import SwiftUI

@main struct DoubaoPTTApp:App {
    @StateObject private var model=AppModel()
    @Environment(\.scenePhase) private var scenePhase
    var body:some Scene {
        WindowGroup {
            ContentView().environmentObject(model)
                .onAppear { model.scene(scenePhase) }
                .onChange(of:scenePhase) { _,value in model.scene(value) }
        }
    }
}
struct ContentView:View {
    @EnvironmentObject private var model:AppModel
    private let blue=Color(red:35/255,green:75/255,blue:198/255)
    var body:some View {
        VStack(spacing:0) {
            HStack {
                Text("语音遥控").font(.system(size:15,weight:.semibold))
                Spacer()
                if model.computer != nil { Button { model.openScanner() } label:{ Image(systemName:"qrcode.viewfinder").font(.system(size:20)) }.accessibilityLabel("重新扫码配对").disabled(model.pairing) }
                Text(model.computer == nil ? "未配对":"已配对").font(.system(size:12)).padding(.horizontal,10).padding(.vertical,6).background(Color(red:0.88,green:0.91,blue:0.95)).clipShape(Capsule())
            }.padding(.horizontal,16).padding(.vertical,14)
            Divider()
            VStack(alignment:.leading,spacing:14) {
                Text(model.computer?.name ?? "连接你的电脑").font(.system(size:27,weight:.semibold)).lineLimit(2)
                Text(model.status).font(.system(size:14)).foregroundStyle(.secondary).frame(minHeight:46,alignment:.topLeading)
                if model.computer==nil { Button("扫码连接电脑") { model.openScanner() }.buttonStyle(.borderedProminent).tint(blue).disabled(model.pairing) }
            }.frame(maxWidth:.infinity,alignment:.leading).padding(16)
            VoiceWaveform(level:model.level,active:model.phase == .priming || model.phase == .streaming)
                .frame(maxWidth:.infinity,minHeight:144,maxHeight:.infinity)
            HoldButton(label:model.buttonLabel,recording:model.phase == .streaming,enabled:model.computer != nil && !model.pairing,onDown:{ model.press() },onUp:{ model.release(above:$0) },onCancel:{ model.cancelTouch() })
                .frame(height:160).padding(.horizontal,16).padding(.bottom,74)
        }
        .background(Color(red:0.95,green:0.96,blue:0.98).ignoresSafeArea())
        .sheet(isPresented:$model.scanner) {
            NavigationStack {
                Scanner(onScan:{model.scan($0)},onError:{model.scanner=false;model.status=$0})
                    .navigationTitle("扫描 App 配对二维码").navigationBarTitleDisplayMode(.inline)
                    .toolbar { ToolbarItem(placement:.cancellationAction) { Button("取消") { model.scanner=false } } }
            }
        }
    }
}

struct VoiceWaveform:View {
    let level:Double
    let active:Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var samples=Array(repeating:0.0,count:21)
    private let blue=Color(red:35/255,green:75/255,blue:198/255)
    var body:some View {
        HStack(alignment:.center,spacing:8) {
            ForEach(samples.indices,id:\.self) { index in
                Capsule().fill(blue.opacity(active ? 0.78:0.25))
                    .frame(width:6,height:barHeight(index))
            }
        }
        .frame(width:300,height:144)
        .animation(reduceMotion ? nil:.easeOut(duration:0.12),value:samples)
        .accessibilityElement(children:.ignore)
        .accessibilityLabel("语音音量")
        .accessibilityValue(active ? "\(Int(min(1,max(0,level))*100))%":"待命")
        .onChange(of:level) { _,value in
            guard active else { return }
            samples.removeFirst();samples.append(min(1,max(0,value)))
        }
        .onChange(of:active) { _,_ in samples=Array(repeating:0,count:21) }
    }
    private func barHeight(_ index:Int) -> CGFloat {
        let envelope=pow(sin(Double(index+1)/Double(samples.count+1)*Double.pi),0.7)
        let sample=samples[samples.count-1-abs(index-samples.count/2)]
        // Boost only the visual response; ignore very low background levels.
        let amplitude=sqrt(min(1,max(0,sample-0.008)*4))
        return 6+CGFloat(amplitude*envelope*128)
    }
}
