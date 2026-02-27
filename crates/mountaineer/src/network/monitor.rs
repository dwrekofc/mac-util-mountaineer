use std::sync::mpsc;
use std::thread;

use core_foundation::array::CFArray;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_foundation::string::CFString;
use system_configuration::dynamic_store::{
    SCDynamicStore, SCDynamicStoreBuilder, SCDynamicStoreCallBackContext,
};

/// Event emitted when macOS detects a network configuration change.
#[derive(Debug)]
pub struct NetworkChangeEvent {
    /// The SCDynamicStore keys that changed (e.g. "State:/Network/Interface/en0/IPv4").
    pub changed_keys: Vec<String>,
}

/// Start the SCDynamicStore network change monitor on a dedicated background thread.
///
/// Returns a receiver that emits [`NetworkChangeEvent`] whenever macOS reports
/// a network configuration change (interface up/down, IP assignment, etc.).
///
/// The background thread runs its own CFRunLoop and lives for the entire
/// application lifetime.
pub fn start() -> mpsc::Receiver<NetworkChangeEvent> {
    let (tx, rx) = mpsc::channel();

    thread::Builder::new()
        .name("network-monitor".into())
        .spawn(move || {
            run_monitor(tx);
        })
        .expect("failed to spawn network monitor thread");

    rx
}

fn run_monitor(tx: mpsc::Sender<NetworkChangeEvent>) {
    let callback_context = SCDynamicStoreCallBackContext {
        callout: sc_callback,
        info: tx,
    };

    let store = SCDynamicStoreBuilder::new("mountaineer-network-monitor")
        .callback_context(callback_context)
        .build();

    // Watch patterns for network state changes:
    //   - Interface IPv4/IPv6 address changes (assigned/removed)
    //   - Interface link state changes (cable plug/unplug)
    //   - Global primary interface/service changes
    let watch_keys: CFArray<CFString> = CFArray::from_CFTypes(&[]);
    let watch_patterns = CFArray::from_CFTypes(&[
        CFString::from("State:/Network/Interface/.*/IPv4"),
        CFString::from("State:/Network/Interface/.*/IPv6"),
        CFString::from("State:/Network/Interface/.*/Link"),
        CFString::from("State:/Network/Global/IPv4"),
        CFString::from("State:/Network/Global/IPv6"),
    ]);

    if !store.set_notification_keys(&watch_keys, &watch_patterns) {
        log::error!("Failed to set SCDynamicStore notification keys");
        return;
    }

    let run_loop_source = store.create_run_loop_source();
    let run_loop = CFRunLoop::get_current();
    run_loop.add_source(&run_loop_source, unsafe { kCFRunLoopCommonModes });

    log::info!("Network change monitor started on background thread");
    CFRunLoop::run_current();
}

fn sc_callback(
    _store: SCDynamicStore,
    changed_keys: CFArray<CFString>,
    tx: &mut mpsc::Sender<NetworkChangeEvent>,
) {
    let keys: Vec<String> = changed_keys.iter().map(|k| k.to_string()).collect();
    log::debug!("SCDynamicStore callback: {:?}", keys);
    let _ = tx.send(NetworkChangeEvent { changed_keys: keys });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn monitor_starts_and_returns_receiver() {
        let rx = start();
        // The monitor is running on a background thread.
        // We can't easily trigger a real network change in a test,
        // but we can verify the receiver is valid and non-blocking.
        assert!(
            rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "should not receive events without network changes"
        );
    }
}
