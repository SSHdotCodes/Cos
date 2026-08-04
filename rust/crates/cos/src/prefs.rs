//! Preferences storage byte-compatible with the Swift app: JSON blobs under
//! `cos.*` keys in the `codes.ssh.cos` UserDefaults domain.

use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_foundation::{NSData, NSString, NSUserDefaults};

fn defaults() -> Retained<NSUserDefaults> {
    unsafe {
        let name = NSString::from_str("codes.ssh.cos");
        NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&name))
            .unwrap_or_else(NSUserDefaults::standardUserDefaults)
    }
}

pub fn load_json(key: &str) -> Option<serde_json::Value> {
    unsafe {
        let full_key = NSString::from_str(&format!("cos.{key}"));
        let data: Option<Retained<NSData>> = defaults().dataForKey(&full_key);
        let data = data?;
        serde_json::from_slice(&data.to_vec()).ok()
    }
}

pub fn save_json(key: &str, value: &serde_json::Value) {
    let Ok(bytes) = serde_json::to_vec(value) else { return };
    unsafe {
        let full_key = NSString::from_str(&format!("cos.{key}"));
        let data = NSData::from_vec(bytes);
        defaults().setObject_forKey(Some(&data), &full_key);
    }
}

pub fn load<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    serde_json::from_value(load_json(key)?).ok()
}

pub fn save<T: serde::Serialize>(key: &str, value: &T) {
    if let Ok(value) = serde_json::to_value(value) {
        save_json(key, &value);
    }
}
