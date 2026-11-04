import SwiftUI
import UIKit

struct HoldSurface:UIViewRepresentable {
    let computerName:String
    let status:String
    let paired:Bool
    let pairing:Bool
    let holding:Bool
    let resetVersion:Int
    let voiceActive:Bool
    let streaming:Bool
    let level:Double
    let onDown:()->Bool
    let onUp:(Bool)->Void
    let onCancel:()->Void
    let onScan:()->Void

    func makeUIView(context:Context) -> HoldSurfaceView { HoldSurfaceView() }
    func updateUIView(_ view:HoldSurfaceView,context:Context) {
        view.onDown=onDown;view.onUp=onUp;view.onCancel=onCancel;view.onScan=onScan
        view.update(computerName:computerName,status:status,paired:paired,pairing:pairing,
                    holding:holding,resetVersion:resetVersion,voiceActive:voiceActive,streaming:streaming,level:level)
    }
    static func dismantleUIView(_ view:HoldSurfaceView,coordinator:()) { view.cancelInteraction() }
}

// All geometry is in this view, which starts immediately below the header divider.
final class HoldSurfaceView:UIView {
    var onDown:(()->Bool)?
    var onUp:((Bool)->Void)?
    var onCancel:(()->Void)?
    var onScan:(()->Void)?
    private static let radius:CGFloat=60
    private static let transitionDuration:CFTimeInterval=0.3
    private let blue=UIColor(red:35/255,green:75/255,blue:198/255,alpha:1)
    private let computerLabel=UILabel(),statusLabel=UILabel(),hintLabel=UILabel()
    private let hintGroup=UIStackView()
    private let statusGroup=UIStackView(),actionList=UIStackView()
    private var actionListVisible:Bool?
    private let scanButton=UIButton(type:.system)
    private let visual=CALayer(),rippleContainer=CALayer(),quietRippleContainer=CALayer()
    private let fill=CAShapeLayer(),outline=CAShapeLayer()
    private var isStreaming=false
    private var baseStatus=""
    private var rings:[CAShapeLayer]=[],quietRings:[CAShapeLayer]=[]
    private weak var primary:UITouch?
    private var gestureActive=false,canRecord=false,voiceActive=false
    private var anchor=CGPoint.zero
    private var targetLevel:Double=0,smoothedLevel:Double=0
    private var displayLink:CADisplayLink?
    private var lastTick:CFTimeInterval=0
    private var resetVersion:Int?
    private var hintVisible:Bool?
    private var visualGeneration=0
    private var ringsRunning=false
    private var reduceMotion=UIAccessibility.isReduceMotionEnabled
    private var previousSize=CGSize.zero

    override init(frame:CGRect) {
        super.init(frame:frame)
        backgroundColor = .clear;isMultipleTouchEnabled=true;clipsToBounds=true
        computerLabel.font = .systemFont(ofSize:27,weight:.semibold);computerLabel.numberOfLines=2
        statusLabel.font = .systemFont(ofSize:14);statusLabel.textColor = .secondaryLabel;statusLabel.numberOfLines=0
        statusGroup.axis = .vertical;statusGroup.alignment = .fill;statusGroup.spacing=10;statusGroup.isUserInteractionEnabled=false
        actionList.axis = .vertical;actionList.alignment = .fill;actionList.spacing=8;actionList.isUserInteractionEnabled=false
        for (slash,text) in [(false,"圆内松手，会自动按 Enter 键。"),(true,"圈外松手，不触发任何按键。")] {
            let icon=ActionKeyIconView(slashed:slash)
            let label=UILabel();label.text=text;label.font = .systemFont(ofSize:14);label.textColor = .secondaryLabel;label.numberOfLines=0
            let row=UIStackView(arrangedSubviews:[icon,label]);row.axis = .horizontal;row.alignment = .center;row.spacing=10
            icon.translatesAutoresizingMaskIntoConstraints=false
            NSLayoutConstraint.activate([icon.widthAnchor.constraint(equalToConstant:24),icon.heightAnchor.constraint(equalToConstant:24)])
            actionList.addArrangedSubview(row)
        }
        statusGroup.addArrangedSubview(statusLabel);statusGroup.addArrangedSubview(actionList)
        hintLabel.text="按住任意位置说话";hintLabel.font = .systemFont(ofSize:17,weight:.medium)
        hintLabel.textColor=blue.withAlphaComponent(0.35);hintLabel.textAlignment = .center
        let hintIcon=UIImageView(image:UIImage(systemName:"mic.fill",withConfiguration:UIImage.SymbolConfiguration(pointSize:42,weight:.regular)))
        hintIcon.tintColor=blue.withAlphaComponent(0.6);hintIcon.contentMode = .scaleAspectFit;hintIcon.isAccessibilityElement=false
        hintGroup.axis = .vertical;hintGroup.alignment = .center;hintGroup.spacing=14;hintGroup.isUserInteractionEnabled=false
        hintGroup.addArrangedSubview(hintIcon);hintGroup.addArrangedSubview(hintLabel)
        hintIcon.translatesAutoresizingMaskIntoConstraints=false
        NSLayoutConstraint.activate([hintIcon.widthAnchor.constraint(equalToConstant:44),hintIcon.heightAnchor.constraint(equalToConstant:48)])
        scanButton.setTitle("扫码连接电脑",for:.normal);scanButton.titleLabel?.font = .systemFont(ofSize:16,weight:.semibold)
        scanButton.backgroundColor=blue;scanButton.setTitleColor(.white,for:.normal);scanButton.layer.cornerRadius=12
        scanButton.addTarget(self,action:#selector(scan),for:.touchUpInside)
        for view in [computerLabel,statusGroup,hintGroup,scanButton] {
            view.translatesAutoresizingMaskIntoConstraints=false;addSubview(view)
        }
        let centered=hintGroup.centerYAnchor.constraint(equalTo:centerYAnchor);centered.priority = .defaultHigh
        NSLayoutConstraint.activate([
            computerLabel.topAnchor.constraint(equalTo:topAnchor,constant:16),
            computerLabel.leadingAnchor.constraint(equalTo:leadingAnchor,constant:16),
            computerLabel.trailingAnchor.constraint(equalTo:trailingAnchor,constant:-16),
            statusGroup.topAnchor.constraint(equalTo:computerLabel.bottomAnchor,constant:14),
            statusGroup.leadingAnchor.constraint(equalTo:computerLabel.leadingAnchor),
            statusGroup.trailingAnchor.constraint(equalTo:computerLabel.trailingAnchor),
            scanButton.topAnchor.constraint(equalTo:statusGroup.bottomAnchor,constant:14),
            scanButton.leadingAnchor.constraint(equalTo:computerLabel.leadingAnchor),
            scanButton.widthAnchor.constraint(equalToConstant:160),scanButton.heightAnchor.constraint(equalToConstant:46),
            hintGroup.centerXAnchor.constraint(equalTo:centerXAnchor),centered,
            hintGroup.topAnchor.constraint(greaterThanOrEqualTo:scanButton.bottomAnchor,constant:24),
            hintGroup.leadingAnchor.constraint(greaterThanOrEqualTo:leadingAnchor,constant:16),
            hintGroup.trailingAnchor.constraint(lessThanOrEqualTo:trailingAnchor,constant:-16)
        ])
        visual.bounds=CGRect(x:0,y:0,width:300,height:300);visual.opacity=0
        for container in [quietRippleContainer,rippleContainer] { container.frame=visual.bounds;visual.addSublayer(container) }
        let circle=CGRect(x:90,y:90,width:120,height:120)
        for shape in [fill,outline] { shape.frame=circle;shape.path=CGPath(ellipseIn:CGRect(origin:.zero,size:circle.size),transform:nil);visual.addSublayer(shape) }
        outline.fillColor=UIColor.clear.cgColor;outline.strokeColor=blue.withAlphaComponent(0.7).cgColor;outline.lineWidth=1.5
        let iconBounds=CGRect(x:0,y:0,width:40,height:44)
        let icon=UIGraphicsImageRenderer(size:iconBounds.size).image { _ in
            if let image=UIImage(systemName:"mic.fill",withConfiguration:UIImage.SymbolConfiguration(pointSize:32,weight:.semibold))?.withTintColor(.white,renderingMode:.alwaysOriginal) {
                let scale=min(iconBounds.width/image.size.width,iconBounds.height/image.size.height)
                let size=CGSize(width:image.size.width*scale,height:image.size.height*scale)
                image.draw(in:CGRect(x:(iconBounds.width-size.width)/2,y:(iconBounds.height-size.height)/2,width:size.width,height:size.height))
            }
        }
        let iconLayer=CALayer();iconLayer.frame=CGRect(x:130,y:128,width:40,height:44)
        iconLayer.contents=icon.cgImage;iconLayer.contentsScale=icon.scale;visual.addSublayer(iconLayer)
        for container in [quietRippleContainer,rippleContainer] {
            let count=container === rippleContainer ? 3:1
            for _ in 0..<count {
                let ring=CAShapeLayer();ring.frame=visual.bounds;ring.fillColor=UIColor.clear.cgColor
                ring.strokeColor=blue.cgColor;ring.lineWidth=4;ring.opacity=0
                container.addSublayer(ring)
                if container === rippleContainer { rings.append(ring) } else { quietRings.append(ring) }
            }
        }
        layer.addSublayer(visual)
        NotificationCenter.default.addObserver(self,selector:#selector(motionChanged),name:UIAccessibility.reduceMotionStatusDidChangeNotification,object:nil)
    }
    required init?(coder:NSCoder) { fatalError("init(coder:) is not supported") }
    @objc private func scan() { onScan?() }
    @objc private func motionChanged() {
        reduceMotion=UIAccessibility.isReduceMotionEnabled
        stopRings();if gestureActive && voiceActive && !reduceMotion { startRings() }
        drawLevel()
    }
    func update(computerName:String,status:String,paired:Bool,pairing:Bool,holding:Bool,resetVersion:Int,voiceActive:Bool,streaming:Bool,level:Double) {
        if let previous=self.resetVersion,previous != resetVersion { cancelInteraction(notify:false) }
        self.resetVersion=resetVersion
        computerLabel.text=computerName;baseStatus=status;isStreaming=streaming
        scanButton.isHidden=paired;scanButton.isEnabled = !pairing;actionList.isHidden = !paired
        canRecord=paired && !pairing;setHintVisible(paired && !holding)
        // A model stop (network failure, permissions, background) never synthesizes a release.
        if gestureActive && (!holding || !canRecord) { cancelInteraction(notify:false) }
        self.voiceActive=voiceActive
        targetLevel=voiceActive && level.isFinite ? sqrt(min(1,max(0,level-0.008)*4)):0
        if gestureActive {
            if voiceActive && !reduceMotion { startRings() } else { stopRings() }
        }
        refreshStatusText()
    }
    private func setHintVisible(_ visible:Bool) {
        guard hintVisible != visible else { return }
        let animate=hintVisible != nil && window != nil
        hintVisible=visible;hintGroup.accessibilityElementsHidden = !visible
        let opacity:Float=visible ? 1:0
        let from=hintGroup.layer.presentation()?.opacity ?? hintGroup.layer.opacity
        hintGroup.layer.removeAnimation(forKey:"hintFade")
        CATransaction.begin();CATransaction.setDisableActions(true);hintGroup.layer.opacity=opacity;CATransaction.commit()
        if animate {
            let fade=CABasicAnimation(keyPath:"opacity");fade.fromValue=from;fade.toValue=opacity;fade.duration=Self.transitionDuration
            fade.timingFunction=CAMediaTimingFunction(name:.easeInEaseOut)
            hintGroup.layer.add(fade,forKey:"hintFade")
        }
    }
    private func refreshStatusText() {
        statusLabel.text=baseStatus
        let visible=isStreaming && gestureActive
        guard actionListVisible != visible else { return }
        let animate=actionListVisible != nil && window != nil
        actionListVisible=visible;actionList.accessibilityElementsHidden = !visible
        let opacity:Float=visible ? 1:0
        let from=actionList.layer.presentation()?.opacity ?? actionList.layer.opacity
        actionList.layer.removeAnimation(forKey:"actionsFade")
        CATransaction.begin();CATransaction.setDisableActions(true);actionList.layer.opacity=opacity;CATransaction.commit()
        if animate {
            let fade=CABasicAnimation(keyPath:"opacity");fade.fromValue=from;fade.toValue=opacity;fade.duration=Self.transitionDuration
            fade.timingFunction=CAMediaTimingFunction(name:.easeInEaseOut)
            actionList.layer.add(fade,forKey:"actionsFade")
        }
    }
    private func isInside(_ point:CGPoint) -> Bool {
        bounds.contains(point) && hypot(point.x-anchor.x,point.y-anchor.y)<=Self.radius
    }
    override func touchesBegan(_ touches:Set<UITouch>,with event:UIEvent?) {
        guard canRecord,primary==nil,let touch=touches.first else { return }
        let point=touch.location(in:self);guard bounds.contains(point) else { return }
        primary=touch;gestureActive=true;anchor=point
        setHintVisible(false);beginVisual()
        if onDown?() != true { cancelInteraction(notify:false) }
    }
    override func touchesMoved(_ touches:Set<UITouch>,with event:UIEvent?) {
        // Both action explanations stay visible; the final position alone selects the action.
    }
    override func touchesEnded(_ touches:Set<UITouch>,with event:UIEvent?) {
        guard let touch=primary,touches.contains(touch) else { return }
        primary=nil
        guard gestureActive else { return }
        let finishOnly = !isInside(touch.location(in:self))
        gestureActive=false;refreshStatusText();finishVisual(animated:true);setHintVisible(canRecord);onUp?(finishOnly)
    }
    override func touchesCancelled(_ touches:Set<UITouch>,with event:UIEvent?) {
        guard let touch=primary,touches.contains(touch) else { return }
        primary=nil;cancelInteraction()
    }
    func cancelInteraction(notify:Bool=true) {
        let shouldNotify=gestureActive;gestureActive=false;refreshStatusText();finishVisual(animated:false)
        setHintVisible(canRecord)
        // Retain the primary identity until its up/cancel: another finger cannot restart a failed hold.
        if notify && shouldNotify { onCancel?() }
    }
    override func didMoveToWindow() { super.didMoveToWindow();if window==nil { cancelInteraction() } }
    override func layoutSubviews() {
        super.layoutSubviews()
        if previousSize != .zero && previousSize != bounds.size { cancelInteraction() }
        previousSize=bounds.size
    }
    private func beginVisual() {
        visualGeneration+=1;visual.removeAllAnimations();stopRings()
        smoothedLevel=0;targetLevel=0;lastTick=0
        CATransaction.begin();CATransaction.setDisableActions(true)
        visual.position=anchor;visual.opacity=1;CATransaction.commit();drawLevel();refreshStatusText()
        let appear=CABasicAnimation(keyPath:"opacity");appear.fromValue=0;appear.toValue=1;appear.duration=Self.transitionDuration
        appear.timingFunction=CAMediaTimingFunction(name:.easeInEaseOut)
        visual.add(appear,forKey:"pressFade")
        let link=CADisplayLink(target:WeakTicker(self),selector:#selector(WeakTicker.tick(_:)))
        link.add(to:.main,forMode:.common);displayLink=link
        if voiceActive && !reduceMotion { startRings() }
    }
    private func finishVisual(animated:Bool) {
        visualGeneration+=1;let generation=visualGeneration
        displayLink?.invalidate();displayLink=nil;targetLevel=0;smoothedLevel=0;lastTick=0
        let from=visual.presentation()?.opacity ?? (visual.animation(forKey:"pressFade") != nil ? 0:visual.opacity)
        visual.removeAllAnimations()
        CATransaction.begin();CATransaction.setDisableActions(true);visual.opacity=0;CATransaction.commit()
        if animated {
            let fade=CABasicAnimation(keyPath:"opacity");fade.fromValue=from;fade.toValue=0;fade.duration=Self.transitionDuration
            fade.timingFunction=CAMediaTimingFunction(name:.easeInEaseOut)
            CATransaction.begin()
            CATransaction.setCompletionBlock { [weak self] in
                guard let self,visualGeneration==generation,!gestureActive else { return };stopRings()
            }
            visual.add(fade,forKey:"releaseFade");CATransaction.commit()
        } else { stopRings() }
    }
    fileprivate func tick(_ link:CADisplayLink) {
        guard gestureActive else { return }
        let dt=lastTick==0 ? 1.0/60:min(0.1,max(0,link.timestamp-lastTick));lastTick=link.timestamp
        let target=voiceActive ? targetLevel:0
        let tau=target>smoothedLevel ? 0.06:0.22
        smoothedLevel+=(target-smoothedLevel)*(1-exp(-dt/tau))
        if target==0 && smoothedLevel<0.001 { smoothedLevel=0 }
        drawLevel()
    }
    private func drawLevel() {
        let amplitude=voiceActive ? smoothedLevel:0
        // Keep the operation circle opaque; volume changes its tint, never its size or alpha.
        let strength=CGFloat(amplitude)
        let lighten:CGFloat=voiceActive ? 0:0.15
        let red=(104-83*strength)/255,green=(139-91*strength)/255,blueComponent=(227-94*strength)/255
        CATransaction.begin();CATransaction.setDisableActions(true)
        fill.fillColor=UIColor(red:red+(1-red)*lighten,green:green+(1-green)*lighten,
                              blue:blueComponent+(1-blueComponent)*lighten,alpha:1).cgColor
        rippleContainer.opacity=reduceMotion ? 0:Float(amplitude*0.6)
        // A small, slow ambient wave crossfades out as speech becomes stronger.
        quietRippleContainer.opacity=reduceMotion || !voiceActive ? 0:Float(0.14*max(0,1-amplitude*3))
        CATransaction.commit()
    }
    private func startRings() {
        guard !ringsRunning else { return };ringsRunning=true
        animateRings(rings,radius:150,duration:1.2)
        animateRings(quietRings,radius:82,duration:1.6,pause:0.4)
    }
    private func animateRings(_ layers:[CAShapeLayer],radius:CGFloat,duration:CFTimeInterval,pause:CFTimeInterval=0) {
        let start=CGPath(ellipseIn:CGRect(x:90,y:90,width:120,height:120),transform:nil)
        let end=CGPath(ellipseIn:CGRect(x:150-radius,y:150-radius,width:radius*2,height:radius*2),transform:nil)
        let now=CACurrentMediaTime()
        for (index,ring) in layers.enumerated() {
            let path=CABasicAnimation(keyPath:"path");path.fromValue=start;path.toValue=end;path.duration=duration
            let opacity=CABasicAnimation(keyPath:"opacity");opacity.fromValue=1;opacity.toValue=0;opacity.duration=duration
            opacity.fillMode = .forwards;opacity.isRemovedOnCompletion=false
            let group=CAAnimationGroup();group.animations=[path,opacity];group.duration=duration+pause
            group.beginTime=now+Double(index)*duration/Double(layers.count);group.repeatCount = .infinity
            group.timingFunction=CAMediaTimingFunction(name:.linear)
            ring.add(group,forKey:"outward")
        }
    }
    private func stopRings() { ringsRunning=false;for ring in rings+quietRings { ring.removeAllAnimations() } }
    deinit { displayLink?.invalidate();NotificationCenter.default.removeObserver(self) }
}

private final class WeakTicker:NSObject {
    weak var owner:HoldSurfaceView?
    init(_ owner:HoldSurfaceView) { self.owner=owner }
    @objc func tick(_ link:CADisplayLink) { owner?.tick(link) }
}

// Original vector icons matching ios/design/key-action-icons.svg; no runtime image dependency.
private final class ActionKeyIconView:UIView {
    private let slashed:Bool
    init(slashed:Bool) {
        self.slashed=slashed;super.init(frame:.zero)
        backgroundColor = .clear;isOpaque=false;isUserInteractionEnabled=false;isAccessibilityElement=false;contentMode = .redraw
    }
    required init?(coder:NSCoder) { fatalError("init(coder:) is not supported") }
    override func draw(_ rect:CGRect) {
        guard let c=UIGraphicsGetCurrentContext() else { return }
        let scale=min(bounds.width/44,bounds.height/48)
        c.saveGState();defer{c.restoreGState()}
        c.translateBy(x:(bounds.width-44*scale)/2,y:(bounds.height-48*scale)/2);c.scaleBy(x:scale,y:scale);c.translateBy(x:-10,y:-8)
        let color=UIColor(red:119/255,green:130/255,blue:147/255,alpha:1).cgColor
        c.setStrokeColor(color);c.setLineWidth(2.4);c.setLineCap(.round);c.setLineJoin(.round)
        c.move(to:CGPoint(x:48,y:16));c.addLine(to:CGPoint(x:48,y:32))
        c.addCurve(to:CGPoint(x:40,y:40),control1:CGPoint(x:48,y:36.418),control2:CGPoint(x:44.418,y:40));c.addLine(to:CGPoint(x:14,y:40))
        c.move(to:CGPoint(x:23,y:31));c.addLine(to:CGPoint(x:14,y:40));c.addLine(to:CGPoint(x:23,y:49));c.strokePath()
        if slashed {
            c.saveGState();c.setBlendMode(.clear);c.setLineWidth(7)
            c.move(to:CGPoint(x:14,y:12));c.addLine(to:CGPoint(x:50,y:52));c.strokePath();c.restoreGState()
            c.setStrokeColor(color);c.setLineWidth(2.4)
            c.move(to:CGPoint(x:14,y:12));c.addLine(to:CGPoint(x:50,y:52));c.strokePath()
        }
    }
}
