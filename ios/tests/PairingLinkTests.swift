// Standalone, read-only parser checks. Compile with Connection.swift on macOS.
// Usage: pairing-link-tests PUBLIC_CA_PEM [--parse-stdin]
import Foundation

@main struct PairingLinkTests {
    static func main() throws {
        let pem=try String(contentsOfFile:CommandLine.arguments[1],encoding:.utf8)
        let base64=pem.split(whereSeparator:\.isNewline).filter{!$0.hasPrefix("---")}.joined()
        let ca=Data(base64Encoded:base64)!
        let encoded=base64.replacingOccurrences(of:"+",with:"-").replacingOccurrences(of:"/",with:"_").replacingOccurrences(of:"=",with:"")
        let ticket=String(repeating:"a",count:43)
        let url="http://192.168.1.20:38441/setup?token=\(ticket)#v=1&port=38443&ca=\(encoded)"
        let parsed=try PairPayload.parse(url)
        let computer=try parsed.record()
        precondition(computer.host=="192.168.1.20" && computer.port==38443 && computer.ca==ca && parsed.ticket==ticket)
        let legacy=try JSONSerialization.data(withJSONObject:["type":"doubao_ptt_pair","version":1,"host":"192.168.1.20","port":38443,"ticket":ticket,"ca_der":base64])
        let old=try PairPayload.parse(String(decoding:legacy,as:UTF8.self)).record()
        precondition(old.ca==ca)
        let invalid=[
            url.replacingOccurrences(of:"http:",with:"https:"),
            url.replacingOccurrences(of:"/setup?",with:"/other?"),
            url.replacingOccurrences(of:"192.168.1.20",with:"127.0.0.1"),
            url.replacingOccurrences(of:"192.168.1.20",with:"user@192.168.1.20"),
            url.replacingOccurrences(of:"#v=1",with:"&token=other#v=1"),
            url.replacingOccurrences(of:"#v=1",with:"#v=2"),
            url+"&v=1",
            url.replacingOccurrences(of:"port=38443",with:"port=443"),
            url.replacingOccurrences(of:"&ca="+encoded,with:""),
            url.replacingOccurrences(of:"ca="+encoded,with:"ca=YWJj"),
            url.replacingOccurrences(of:"ca="+encoded,with:"ca=***"),
            String(repeating:"x",count:16385),
        ]
        for value in invalid {
            do { _=try PairPayload.parse(value);fatalError("invalid pairing link was accepted") }
            catch { /* Expected; no request or trust override is performed. */ }
        }
        print("PASS: unified link and legacy JSON; rejected \(invalid.count) invalid inputs")
        if CommandLine.arguments.contains("--parse-stdin") {
            let input=FileHandle.standardInput.readDataToEndOfFile()
            let real=try PairPayload.parse(String(decoding:input,as:UTF8.self)).record()
            precondition(real.ca==ca)
            print("PASS: actual desktop-generated link and pinned CA")
        }
    }
}
