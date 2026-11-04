import Foundation
import CoreGraphics
import ImageIO
let output=CommandLine.arguments.dropFirst().first ?? "DoubaoPTT/Assets.xcassets/AppIcon.appiconset/AppIcon.png"
guard let context=CGContext(data:nil,width:1024,height:1024,bitsPerComponent:8,bytesPerRow:4096,space:CGColorSpaceCreateDeviceRGB(),bitmapInfo:CGImageAlphaInfo.noneSkipLast.rawValue) else { fatalError("Cannot create icon bitmap") }
// iOS applies its own corner mask; the blue background must cover the entire asset.
context.setFillColor(CGColor(red:35/255,green:75/255,blue:198/255,alpha:1));context.fill(CGRect(x:0,y:0,width:1024,height:1024))
context.translateBy(x:0,y:1024);context.scaleBy(x:16,y:-16)
context.translateBy(x:0,y:-2.5);context.setFillColor(CGColor(red:1,green:1,blue:1,alpha:1));context.setStrokeColor(CGColor(red:1,green:1,blue:1,alpha:1))
let mic=CGPath(roundedRect:CGRect(x:24,y:13,width:16,height:27),cornerWidth:8,cornerHeight:8,transform:nil);context.addPath(mic);context.fillPath()
context.setLineWidth(4);context.setLineCap(.round);context.setLineJoin(.round)
context.move(to:CGPoint(x:19,y:30));context.addLine(to:CGPoint(x:19,y:34));context.addCurve(to:CGPoint(x:32,y:47),control1:CGPoint(x:19,y:41.18),control2:CGPoint(x:24.82,y:47));context.addCurve(to:CGPoint(x:45,y:34),control1:CGPoint(x:39.18,y:47),control2:CGPoint(x:45,y:41.18));context.addLine(to:CGPoint(x:45,y:30));context.move(to:CGPoint(x:32,y:47));context.addLine(to:CGPoint(x:32,y:54));context.move(to:CGPoint(x:25,y:54));context.addLine(to:CGPoint(x:39,y:54));context.strokePath()
guard let image=context.makeImage(),let destination=CGImageDestinationCreateWithURL(URL(fileURLWithPath:output) as CFURL,"public.png" as CFString,1,nil) else { fatalError("Cannot create icon PNG") }
CGImageDestinationAddImage(destination,image,nil);guard CGImageDestinationFinalize(destination) else { fatalError("Cannot write icon PNG") }
print("Generated opaque iOS app icon without a graphical session")
