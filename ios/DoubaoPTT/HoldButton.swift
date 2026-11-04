import SwiftUI
import UIKit

struct HoldButton:UIViewRepresentable {
    let label:String
    let recording:Bool
    let enabled:Bool
    let onDown:()->Void
    let onUp:(Bool)->Void
    let onCancel:()->Void
    func makeUIView(context:Context) -> HoldControl { HoldControl() }
    func updateUIView(_ view:HoldControl,context:Context) {
        view.title.text=label;view.isUserInteractionEnabled=enabled;view.alpha=enabled ? 1:0.5
        view.backgroundColor=recording ? UIColor(red:0.09,green:0.25,blue:0.22,alpha:1):UIColor(red:35/255,green:75/255,blue:198/255,alpha:1)
        view.onDown=onDown;view.onUp=onUp;view.onCancel=onCancel;view.accessibilityLabel=label
    }
}
final class HoldControl:UIView {
    let title=UILabel()
    var onDown:(()->Void)?
    var onUp:((Bool)->Void)?
    var onCancel:(()->Void)?
    private weak var primary:UITouch?
    override init(frame:CGRect) {
        super.init(frame:frame);layer.cornerRadius=30;isMultipleTouchEnabled=false;isAccessibilityElement=true;accessibilityTraits = .button
        let image=UIImageView(image:UIImage(systemName:"mic.fill",withConfiguration:UIImage.SymbolConfiguration(pointSize:30,weight:.medium)));image.tintColor = .white;image.contentMode = .scaleAspectFit
        title.textColor = .white;title.font = .systemFont(ofSize:26,weight:.semibold);title.textAlignment = .center
        let hint=UILabel();hint.text="按住说话";hint.textColor=UIColor.white.withAlphaComponent(0.8);hint.font = .systemFont(ofSize:14);hint.textAlignment = .center
        let stack=UIStackView(arrangedSubviews:[image,title]);stack.axis = .vertical;stack.alignment = .center;stack.spacing=12;stack.isUserInteractionEnabled=false;stack.translatesAutoresizingMaskIntoConstraints=false;addSubview(stack)
        NSLayoutConstraint.activate([stack.centerXAnchor.constraint(equalTo:centerXAnchor),stack.centerYAnchor.constraint(equalTo:centerYAnchor),image.heightAnchor.constraint(equalToConstant:34)])
    }
    required init?(coder:NSCoder) { fatalError("init(coder:) is not supported") }
    override func touchesBegan(_ touches:Set<UITouch>,with event:UIEvent?) {
        guard primary==nil,let touch=touches.first else { return };primary=touch;onDown?()
    }
    override func touchesEnded(_ touches:Set<UITouch>,with event:UIEvent?) {
        guard let touch=primary,touches.contains(touch) else { return }
        let above=touch.location(in:self).y<0;primary=nil;onUp?(above)
    }
    override func touchesMoved(_ touches:Set<UITouch>,with event:UIEvent?) {
        // UIKit keeps delivering this primary touch outside the view; release uses its final Y.
    }
    override func touchesCancelled(_ touches:Set<UITouch>,with event:UIEvent?) {
        guard let touch=primary,touches.contains(touch) else { return };primary=nil;onCancel?()
    }
    override func didMoveToWindow() {
        if window==nil && primary != nil { primary=nil;onCancel?() }
    }
}
