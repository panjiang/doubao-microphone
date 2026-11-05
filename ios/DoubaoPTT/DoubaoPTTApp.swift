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
    var body:some View {
        VStack(spacing:0) {
            HStack {
                Text("豆包遥控").font(.system(size:15,weight:.semibold))
                Spacer()
                if model.computer != nil { Button { model.openScanner() } label:{ Image(systemName:"qrcode.viewfinder").font(.system(size:20)) }.accessibilityLabel("重新扫码配对").disabled(model.pairing || model.holding) }
                Text(model.computer == nil ? "未配对":"已配对").font(.system(size:12)).padding(.horizontal,10).padding(.vertical,6).background(Color(red:0.88,green:0.91,blue:0.95)).clipShape(Capsule())
            }.padding(.horizontal,16).padding(.vertical,14)
            Divider()
            HoldSurface(computerName:model.computer?.name ?? "连接你的电脑",status:model.status,
                        paired:model.computer != nil,pairing:model.pairing,holding:model.holding,resetVersion:model.generation,
                        voiceActive:model.phase == .priming || model.phase == .streaming,
                        streaming:model.phase == .streaming,level:model.level,
                        onDown:{ model.press();return model.holding },
                        onUp:{ model.release(finishOnly:$0) },onCancel:{ model.cancelTouch() },
                        onScan:{ model.openScanner() })
                .frame(maxWidth:.infinity,maxHeight:.infinity)
        }
        .background(Color(red:0.95,green:0.96,blue:0.98).ignoresSafeArea())
        .sheet(isPresented:$model.scanner) {
            NavigationStack {
                Scanner(onScan:{model.scan($0)},onError:{model.scanner=false;model.status=$0})
                    .navigationTitle("扫描配对二维码").navigationBarTitleDisplayMode(.inline)
                    .toolbar { ToolbarItem(placement:.cancellationAction) { Button("取消") { model.scanner=false } } }
            }
        }
    }
}
