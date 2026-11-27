//! Tiny persisted preferences (%APPDATA%\ear-desktop\settings.json).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Settings {
    /// Last device the user connected to; reconnected automatically on launch.
    pub last_address: Option<u64>,
    /// Bluetooth address (hex) -> Fast Pair SKU id, so model detection is skipped next time.
    pub skus: BTreeMap<String, String>,
    pub show_all_devices: bool,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("ear-desktop").join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        let data = serde_json::to_vec_pretty(self).unwrap_or_default();
        // write off the UI thread; it's tiny but disk can stall
        std::thread::spawn(move || {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, data);
        });
    }

    pub fn sku_for(&self, address: u64) -> Option<&str> {
        self.skus.get(&format!("{address:012X}")).map(String::as_str)
    }

    pub fn remember(&mut self, address: u64, sku: &str) {
        self.last_address = Some(address);
        self.skus.insert(format!("{address:012X}"), sku.to_string());
        self.save();
    }
}
