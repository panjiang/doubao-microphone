// Public Apple APIs only. The input-agent owns all retained AX references.
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <SystemConfiguration/SystemConfiguration.h>
#import <Carbon/Carbon.h>
#include <libproc.h>
#include <net/if.h>

typedef struct { AXUIElementRef window; AXUIElementRef focus; pid_t pid; } PTTTarget;

static AXUIElementRef copyElement(AXUIElementRef owner, CFStringRef name) {
    CFTypeRef value = NULL;
    AXError error = AXUIElementCopyAttributeValue(owner, name, &value);
    if (error != kAXErrorSuccess) {
        NSLog(@"DoubaoPTT focus_capture attribute=%@ error=%d", (__bridge NSString *)name, (int)error);
        return NULL;
    }
    if (!value || CFGetTypeID(value) != AXUIElementGetTypeID()) {
        if (value) CFRelease(value);
        return NULL;
    }
    return (AXUIElementRef)value;
}

void ptt_mac_release_target(void *raw) {
    PTTTarget *target = raw;
    if (!target) return;
    if (target->window) CFRelease(target->window);
    if (target->focus) CFRelease(target->focus);
    free(target);
}

void *ptt_mac_capture_target(void) {
    if (!AXIsProcessTrusted()) return NULL;
    @autoreleasepool {
        // System-wide AXFocusedApplication fails on some macOS sessions. Resolve
        // the public workspace PID, then query that application's AX object.
        pid_t pid = NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier;
        if (pid <= 0) return NULL;
        AXUIElementRef app = AXUIElementCreateApplication(pid);
        if (!app) return NULL;
        PTTTarget *target = calloc(1, sizeof(PTTTarget));
        if (target) {
            // At most two remote AX attribute queries, each bounded to 80ms.
            AXUIElementSetMessagingTimeout(app, 0.08f);
            target->pid = pid;
            target->window = copyElement(app, kAXFocusedWindowAttribute);
            target->focus = copyElement(app, kAXFocusedUIElementAttribute);
            pid_t focusPid = 0;
            if (target->focus) AXUIElementGetPid(target->focus, &focusPid);
            pid_t still = NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier;
            bool valid = still == pid && focusPid == pid && target->window && target->focus;
            if (!valid) {
                NSLog(@"DoubaoPTT focus_capture invalid window=%d focus=%d same_app=%d same_pid=%d", target->window != NULL, target->focus != NULL, still == pid, focusPid == pid);
                ptt_mac_release_target(target); target = NULL;
            }
        }
        CFRelease(app);
        return target;
    }
}

int ptt_mac_tap_enter(void *raw) {
    if (!AXIsProcessTrusted()) return 4;
    PTTTarget *target = raw;
    if (!target) return 3;
    PTTTarget *now = ptt_mac_capture_target();
    if (!now) return 1;
    bool same = target->pid == now->pid && CFEqual(target->window, now->window) && CFEqual(target->focus, now->focus);
    ptt_mac_release_target(now);
    if (!same) return 1;
    CGEventFlags modifiers = kCGEventFlagMaskShift | kCGEventFlagMaskControl | kCGEventFlagMaskAlternate | kCGEventFlagMaskCommand | kCGEventFlagMaskSecondaryFn;
    // Check both physical state and combined state; never release a user's key.
    if ((CGEventSourceFlagsState(kCGEventSourceStateHIDSystemState) | CGEventSourceFlagsState(kCGEventSourceStateCombinedSessionState)) & modifiers) return 2;
    for (int state = kCGEventSourceStateCombinedSessionState; state <= kCGEventSourceStateHIDSystemState; state++) {
        if (CGEventSourceKeyState(state, 36) || CGEventSourceKeyState(state, 76)) return 2;
    }
    CGEventRef down = CGEventCreateKeyboardEvent(NULL, 36, true);
    CGEventRef up = CGEventCreateKeyboardEvent(NULL, 36, false);
    if (!down || !up) { if (down) CFRelease(down); if (up) CFRelease(up); return 5; }
    CGEventSetFlags(down, 0); CGEventSetFlags(up, 0);
    CGEventPost(kCGHIDEventTap, down); CGEventPost(kCGHIDEventTap, up);
    CFRelease(down); CFRelease(up);
    return 0;
}

uint8_t ptt_mac_network_rank(uint32_t index) {
    char name[IF_NAMESIZE];
    if (!if_indextoname(index, name)) return 3;
    CFArrayRef interfaces = SCNetworkInterfaceCopyAll();
    if (!interfaces) return 3;
    uint8_t rank = 3;
    for (CFIndex i = 0; i < CFArrayGetCount(interfaces); i++) {
        SCNetworkInterfaceRef item = (SCNetworkInterfaceRef)CFArrayGetValueAtIndex(interfaces, i);
        CFStringRef bsd = SCNetworkInterfaceGetBSDName(item);
        char text[IF_NAMESIZE];
        if (!bsd || !CFStringGetCString(bsd, text, sizeof(text), kCFStringEncodingUTF8) || strcmp(name, text)) continue;
        CFStringRef type = SCNetworkInterfaceGetInterfaceType(item);
        if (type && CFEqual(type, kSCNetworkInterfaceTypeEthernet)) rank = 0;
        else if (type && CFEqual(type, kSCNetworkInterfaceTypeIEEE80211)) rank = 1;
        else rank = 2;
        break;
    }
    CFRelease(interfaces); return rank;
}

int ptt_mac_installation(void) {
    @autoreleasepool {
        NSArray<NSString *> *paths = @[@"/Library/Input Methods/DoubaoIme.app", [NSHomeDirectory() stringByAppendingPathComponent:@"Library/Input Methods/DoubaoIme.app"]];
        NSMutableArray<NSString *> *executables = [NSMutableArray array];
        for (NSString *path in paths) {
            NSBundle *bundle = [NSBundle bundleWithPath:path];
            if ([bundle.bundleIdentifier isEqualToString:@"com.bytedance.inputmethod.doubaoime"] && bundle.executablePath && [[NSFileManager defaultManager] isExecutableFileAtPath:bundle.executablePath]) [executables addObject:bundle.executablePath];
        }
        if (!executables.count) return 0;
        int count = proc_listallpids(NULL, 0);
        if (count <= 0 || count > 100000) return -1;
        int capacity = count + 128;
        pid_t *pids = calloc(capacity, sizeof(pid_t));
        if (!pids) return -1;
        count = proc_listallpids(pids, capacity * sizeof(pid_t));
        if (count > capacity) count = capacity;
        bool running = false;
        for (int i = 0; i < count; i++) {
            char path[PROC_PIDPATHINFO_MAXSIZE];
            if (proc_pidpath(pids[i], path, sizeof(path)) > 0 && [executables containsObject:@(path)]) { running = true; break; }
        }
        free(pids); return running ? 2 : 1;
    }
}

char *ptt_mac_decode_settings(const uint8_t *bytes, size_t length) {
    @autoreleasepool {
        if (!bytes || length > 1024 * 1024) return NULL;
        NSData *data = [NSData dataWithBytes:bytes length:length];
        // Only Foundation value classes may be instantiated; never arbitrary vendor classes.
        NSSet *classes = [NSSet setWithObjects:NSDictionary.class, NSArray.class, NSString.class, NSNumber.class, NSData.class, NSNull.class, nil];
        NSError *error = nil;
        id object = [NSKeyedUnarchiver unarchivedObjectOfClasses:classes fromData:data error:&error];
        if (error || ![object isKindOfClass:NSDictionary.class]) return NULL;
        NSArray *keys = @[@"selectedMicrophoneId", @"asrLongPressShortcutKeyCode", @"asrLongPressShortcutModifierFlags", @"asrLongPressShortcutKeyDisplay", @"isStartASRShortcutEnable", @"isGloableASRShortcutEnable"];
        NSMutableDictionary *allowed = [NSMutableDictionary dictionary];
        for (NSString *key in keys) {
            id value = object[key];
            if ([value isKindOfClass:NSNumber.class] || ([value isKindOfClass:NSString.class] && [value length] <= 256)) allowed[key] = value;
        }
        NSData *json = [NSJSONSerialization dataWithJSONObject:allowed options:0 error:&error];
        if (!json || error) return NULL;
        char *result = malloc(json.length + 1);
        if (result) { memcpy(result, json.bytes, json.length); result[json.length] = 0; }
        return result;
    }
}
void ptt_mac_free_string(char *value) { free(value); }

bool ptt_mac_doubao_selected(void) {
    TISInputSourceRef source = TISCopyCurrentKeyboardInputSource();
    if (!source) return false;
    CFStringRef id = TISGetInputSourceProperty(source, kTISPropertyInputSourceID);
    bool selected = id && CFStringHasPrefix(id, CFSTR("com.bytedance.inputmethod.doubaoime"));
    CFRelease(source); return selected;
}
