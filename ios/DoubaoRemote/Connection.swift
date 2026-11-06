import Foundation
import Security

enum RemoteError: LocalizedError {
    case message(String)
    var errorDescription: String? { if case .message(let text)=self { return text }; return nil }
}

struct PairPayload: Decodable {
    let type: String
    let version: Int
    let host: String
    let port: Int
    let ticket: String
    let ca_der: String
    static func parse(_ text:String) throws -> PairPayload {
        guard text.utf8.count<=16384 else { throw RemoteError.message("二维码数据过大或无效。") }
        if text.first=="{" {
            let payload=try JSONDecoder().decode(PairPayload.self,from:Data(text.utf8))
            _=try payload.record();return payload // Older desktop JSON QR codes remain valid.
        }
        guard let link=URLComponents(string:text),link.scheme?.lowercased()=="http",
              link.user==nil,link.password==nil,link.path=="/setup",let host=link.host,
              let bootstrap=link.port,(1024...65535).contains(bootstrap),
              let query=link.queryItems,query.count==1,query[0].name=="token",let ticket=query[0].value,
              let fragment=link.fragment,
              let fields=URLComponents(string:"?"+fragment)?.queryItems,
              fields.count==3,Set(fields.map(\.name))==Set(["v","port","ca"]),
              fields.allSatisfy({$0.value != nil}) else {
            throw RemoteError.message("请扫描电脑配置页的配对二维码。")
        }
        let values=Dictionary(uniqueKeysWithValues:fields.map{($0.name,$0.value!)})
        guard values["v"]=="1",let port=Int(values["port"]!),let encoded=values["ca"],
              encoded.utf8.allSatisfy({(65...90).contains($0)||(97...122).contains($0)||(48...57).contains($0)||$0==45||$0==95}),encoded.count%4 != 1 else {
            throw RemoteError.message("配对二维码格式无法识别，请在电脑上重新生成。")
        }
        let base64=encoded.replacingOccurrences(of:"-",with:"+").replacingOccurrences(of:"_",with:"/")
            + String(repeating:"=",count:(4-encoded.count%4)%4)
        let payload=PairPayload(type:"doubao_ptt_pair",version:1,host:host,port:port,ticket:ticket,ca_der:base64)
        _=try payload.record();return payload
    }
    func record() throws -> Computer {
        guard type == "doubao_ptt_pair", version == 1, ticket.count >= 32, ticket.count <= 256,
              let ca=Data(base64Encoded: ca_der), ca.count <= 8192,
              SecCertificateCreateWithData(nil,ca as CFData) != nil else {
            throw RemoteError.message("请扫描电脑配置页的配对二维码。")
        }
        let computer=Computer(host:host,port:port,ca:ca,token:"",name:"我的电脑")
        try computer.validate(); return computer
    }
}

struct Computer: Codable {
    var host: String
    var port: Int
    var ca: Data
    var token: String
    var name: String
    func validate() throws {
        let parts=host.split(separator:".",omittingEmptySubsequences:false)
        guard parts.count==4,parts.allSatisfy({ !$0.isEmpty && $0.allSatisfy(\.isNumber) && UInt8($0) != nil }),
              (1024...65535).contains(port),!host.hasPrefix("127."),host != "0.0.0.0",
              SecCertificateCreateWithData(nil,ca as CFData) != nil else {
            throw RemoteError.message("电脑连接信息无效，请在电脑上选择可连接的局域网 IP 后重新扫码。")
        }
    }
    func url(_ path:String,websocket:Bool=false) -> URL {
        var value=URLComponents();value.scheme=websocket ? "wss":"https";value.host=host;value.port=port;value.path=path
        return value.url!
    }
}

enum PairStore {
    private static let query:[String:Any]=[kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:"local.doubao-ptt.ios",kSecAttrAccount as String:"computer"]
    static func load() throws -> Computer? {
        var q=query;q[kSecReturnData as String]=true;q[kSecMatchLimit as String]=kSecMatchLimitOne
        var result:CFTypeRef?;let status=SecItemCopyMatching(q as CFDictionary,&result)
        if status==errSecItemNotFound { return nil }
        guard status==errSecSuccess,let data=result as? Data else { throw RemoteError.message("无法读取已保存的配对，请解锁手机后重试。") }
        let computer=try JSONDecoder().decode(Computer.self,from:data);try computer.validate();return computer
    }
    static func save(_ computer:Computer) throws {
        let data=try JSONEncoder().encode(computer)
        let updates:[String:Any]=[kSecValueData as String:data]
        var status=SecItemUpdate(query as CFDictionary,updates as CFDictionary)
        if status==errSecItemNotFound {
            var q=query;q[kSecValueData as String]=data;q[kSecAttrAccessible as String]=kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            status=SecItemAdd(q as CFDictionary,nil)
        }
        guard status==errSecSuccess else { throw RemoteError.message("无法保存配对，请解锁手机后重试。") }
    }
}

final class PinnedTrust: NSObject,URLSessionDelegate,URLSessionTaskDelegate,@unchecked Sendable {
    let computer:Computer
    init(_ computer:Computer) { self.computer=computer }
    func urlSession(_ session:URLSession,didReceive challenge:URLAuthenticationChallenge,completionHandler:@escaping(URLSession.AuthChallengeDisposition,URLCredential?)->Void) {
        guard challenge.protectionSpace.authenticationMethod==NSURLAuthenticationMethodServerTrust,
              challenge.protectionSpace.host==computer.host,challenge.protectionSpace.port==computer.port,
              let trust=challenge.protectionSpace.serverTrust,
              let ca=SecCertificateCreateWithData(nil,computer.ca as CFData) else {
            completionHandler(.cancelAuthenticationChallenge,nil);return
        }
        let policy=SecPolicyCreateSSL(true,computer.host as CFString)
        guard SecTrustSetPolicies(trust,policy)==errSecSuccess,
              SecTrustSetAnchorCertificates(trust,[ca] as CFArray)==errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(trust,true)==errSecSuccess,
              SecTrustEvaluateWithError(trust,nil) else {
            completionHandler(.cancelAuthenticationChallenge,nil);return
        }
        completionHandler(.useCredential,URLCredential(trust:trust))
    }
    func urlSession(_ session:URLSession,task:URLSessionTask,willPerformHTTPRedirection response:HTTPURLResponse,newRequest request:URLRequest,completionHandler:@escaping(URLRequest?)->Void) { completionHandler(nil) }
}

func pinnedSession(_ computer:Computer) -> URLSession {
    let config=URLSessionConfiguration.ephemeral
    config.timeoutIntervalForRequest=8;config.timeoutIntervalForResource=300
    config.urlCache=nil;config.httpCookieStorage=nil;config.requestCachePolicy = .reloadIgnoringLocalCacheData
    return URLSession(configuration:config,delegate:PinnedTrust(computer),delegateQueue:nil)
}

final class PairingClient {
    let computer:Computer
    private let session:URLSession
    init(_ computer:Computer) { self.computer=computer;session=pinnedSession(computer) }
    func close() { session.invalidateAndCancel() }
    deinit { close() }
    private func json(_ path:String,body:[String:Any]?=nil,query:[URLQueryItem]=[]) async throws -> [String:Any] {
        var components=URLComponents(url:computer.url(path),resolvingAgainstBaseURL:false)!;components.queryItems=query.isEmpty ? nil:query
        var request=URLRequest(url:components.url!)
        if let body { request.httpMethod="POST";request.setValue("application/json",forHTTPHeaderField:"Content-Type");request.httpBody=try JSONSerialization.data(withJSONObject:body) }
        let (data,response)=try await session.data(for:request)
        guard data.count <= 16384,let http=response as? HTTPURLResponse,let value=try JSONSerialization.jsonObject(with:data) as? [String:Any] else { throw RemoteError.message("电脑返回的数据无法识别。") }
        guard http.statusCode==200 else { throw RemoteError.message(value["error"] as? String ?? "电脑拒绝了配对，请重新生成二维码。") }
        return value
    }
    func pair(ticket:String) async throws -> Computer {
        let info=try await json("/info")
        let response=try await json("/native/pair",body:["ticket":ticket,"label":"iPhone App"])
        guard let id=response["id"] as? String,UUID(uuidString:id) != nil,let poll=response["poll"] as? String else { throw RemoteError.message("配对响应无法识别。") }
        let deadline=Date().addingTimeInterval(300)
        while Date()<deadline {
            try Task.checkCancellation()
            let result=try await json("/native/pair-result/"+id,query:[URLQueryItem(name:"poll",value:poll)])
            if let token=result["token"] as? String,!token.isEmpty {
                var saved=computer;saved.token=token;saved.name=info["computer_name"] as? String ?? "我的电脑";return saved
            }
            try await Task.sleep(nanoseconds:750_000_000)
        }
        throw RemoteError.message("配对已超时，请在电脑上重新生成二维码。")
    }
}
