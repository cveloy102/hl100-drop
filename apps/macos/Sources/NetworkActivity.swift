import Foundation

// Rust's BSD sockets are not NSURLSession tasks. Declare the user-requested
// reception/transfer work explicitly so App Nap cannot stall its timers or I/O.
final class NetworkActivity {
    private var listening: NSObjectProtocol?
    private var transfer: NSObjectProtocol?
    func setListening(_ enabled: Bool) {
        if enabled && listening == nil {
            listening = ProcessInfo.processInfo.beginActivity(options: .userInitiatedAllowingIdleSystemSleep,
                                                              reason: "HL100 Drop: automatic LAN reception")
        } else if !enabled, let token = listening {
            ProcessInfo.processInfo.endActivity(token); listening = nil
        }
    }
    func setTransferring(_ enabled: Bool) {
        if enabled && transfer == nil {
            transfer = ProcessInfo.processInfo.beginActivity(options: .userInitiated,
                                                             reason: "HL100 Drop: file transfer in progress")
        } else if !enabled, let token = transfer {
            ProcessInfo.processInfo.endActivity(token); transfer = nil
        }
    }
    func stop() { setTransferring(false); setListening(false) }
    deinit { stop() }
}
