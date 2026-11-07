import SwiftUI
import AVFoundation

struct Scanner:UIViewControllerRepresentable {
    let onScan:(String)->Void
    let onError:(String)->Void
    func makeUIViewController(context:Context) -> ScannerController {
        let controller=ScannerController();controller.onScan=onScan;controller.onError=onError;return controller
    }
    func updateUIViewController(_ controller:ScannerController,context:Context) {}
}
final class ScannerController:UIViewController,AVCaptureMetadataOutputObjectsDelegate {
    var onScan:((String)->Void)?
    var onError:((String)->Void)?
    private let capture=AVCaptureSession()
    private let work=DispatchQueue(label:"ptt.qr")
    private var preview:AVCaptureVideoPreviewLayer?
    private let stateLock=NSLock()
    private var stopped=false
    private var finished:Bool { stateLock.lock();defer{stateLock.unlock()};return stopped }
    private func stopScanning() { stateLock.lock();stopped=true;stateLock.unlock() }
    override func viewDidLoad() {
        super.viewDidLoad();view.backgroundColor = .black
        AVCaptureDevice.requestAccess(for:.video) { [weak self] granted in
            guard let self else { return }
            if granted { work.async { [weak self] in self?.configure() } }
            else { DispatchQueue.main.async { [weak self] in self?.onError?("请在 iPhone 设置中允许相机权限后重新扫码。") } }
        }
    }
    private func configure() {
        guard !finished,let device=AVCaptureDevice.default(for:.video) else { return }
        do {
            let input=try AVCaptureDeviceInput(device:device);let output=AVCaptureMetadataOutput()
            guard capture.canAddInput(input),capture.canAddOutput(output) else { throw RemoteError.message("无法启动扫码相机。") }
            capture.beginConfiguration();capture.addInput(input);capture.addOutput(output)
            output.setMetadataObjectsDelegate(self,queue:.main);output.metadataObjectTypes=[.qr];capture.commitConfiguration()
            DispatchQueue.main.async { [weak self] in
                guard let self,!finished else { return };let layer=AVCaptureVideoPreviewLayer(session:capture);layer.videoGravity = .resizeAspectFill;view.layer.addSublayer(layer);preview=layer;view.setNeedsLayout()
            }
            if !finished { capture.startRunning() }
        } catch { DispatchQueue.main.async { [weak self] in self?.onError?("无法启动扫码相机，请稍后重试。") } }
    }
    override func viewDidLayoutSubviews() { super.viewDidLayoutSubviews();preview?.frame=view.bounds }
    override func viewDidDisappear(_ animated:Bool) { super.viewDidDisappear(animated);stopScanning();work.async { [self] in capture.stopRunning() } }
    func metadataOutput(_ output:AVCaptureMetadataOutput,didOutput metadataObjects:[AVMetadataObject],from connection:AVCaptureConnection) {
        guard !finished,let value=(metadataObjects.first as? AVMetadataMachineReadableCodeObject)?.stringValue else { return }
        stopScanning();onScan?(value);work.async { [self] in capture.stopRunning() }
    }
}
