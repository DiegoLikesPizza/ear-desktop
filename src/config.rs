//! Per-model / per-firmware capability table (ear_config_file.json from ear-web,
//! itself mirrored from the official app). Only the fields the UI uses are typed.

use serde::Deserialize;
use std::sync::OnceLock;

static RAW: &str = include_str!("../assets/ear_config.json");

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    configs: Vec<FirmwareConfig>,
}

#[derive(Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct FirmwareConfig {
    pub min_firmware: Option<String>,
    pub max_firmware: Option<String>,
    pub anc_level: Option<u32>,
    pub high_quality_audio: u32,
    pub dual_connection: u32,
    pub dual_connection_reboot: u32,
    pub spatial_audio: u32,
    pub use_audiodo_spatial: u32,
    pub mimi: u32,
    pub audiodo: u32,
    pub super_mic: u32,
    pub call_transparency: u32,
    pub walkie_talkie_mode: u32,
    pub long_power_mode: u32,
    pub support_leakage_protection: u32,
    pub support_studio: u32,
    pub mutually_exclusive: u32,
    pub dirac_opteo_support: u32,
    pub ear_tip_fit_test: u32,
    #[serde(rename = "customEQ")]
    pub custom_eq: Option<CustomEqSpec>,
    pub controls: Option<Controls>,
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CustomEqSpec {
    pub freq_low: f32,
    pub q_low: f32,
    pub freq_peak: f32,
    pub q_peak: f32,
    pub freq_high: f32,
    pub q_high: f32,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Controls {
    pub ear_gesture_groups: Vec<GestureGroup>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GestureGroup {
    pub group_name: Option<String>,
    pub gestures: Vec<Gesture>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Gesture {
    pub is_custom: bool,
    pub device_type: i32,
    pub button: i32,
    /// negative ids are informational entries (e.g. "roll = volume") with no wire value
    pub gesture: i32,
    pub gesture_name_key: String,
    pub operation_groups: Vec<OperationGroup>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OperationGroup {
    pub group_name: String,
    pub operations: Vec<Operation>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Operation {
    pub operation: i32,
    pub is_custom: bool,
    pub support_platform: Option<u32>,
    pub limit_version: Option<String>,
    pub operation_desc_key: Vec<String>,
}

fn table() -> &'static Vec<ModelEntry> {
    static T: OnceLock<Vec<ModelEntry>> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(RAW).expect("embedded ear_config.json is valid"))
}

/// Pick the config block for a model id (falling back to `base`) and firmware version.
pub fn lookup(config_id: &str, base: &str, firmware: &str) -> Option<FirmwareConfig> {
    let t = table();
    let entry = t.iter().find(|m| m.id == config_id).or_else(|| t.iter().find(|m| m.id == base))?;
    if entry.configs.len() == 1 {
        return entry.configs.first().cloned();
    }
    entry
        .configs
        .iter()
        .find(|c| version::in_range(firmware, c.min_firmware.as_deref(), c.max_firmware.as_deref()))
        .cloned()
}

/// Warm the parse off the UI thread so the first connect doesn't pay for it.
pub fn preload() {
    std::thread::spawn(|| {
        table();
    });
}

pub mod version {
    use std::cmp::Ordering;

    fn parts(v: &str) -> Vec<&str> {
        v.split('-').next().unwrap_or(v).split('.').collect()
    }

    /// Numeric dotted compare; `*` components match anything.
    pub fn compare(a: &str, b: &str) -> Ordering {
        let (pa, pb) = (parts(a), parts(b));
        for i in 0..pa.len().max(pb.len()) {
            let x = pa.get(i).copied().unwrap_or("0");
            let y = pb.get(i).copied().unwrap_or("0");
            if x == "*" || y == "*" {
                continue;
            }
            let (x, y) = (x.trim().parse::<u64>().unwrap_or(0), y.trim().parse::<u64>().unwrap_or(0));
            match x.cmp(&y) {
                Ordering::Equal => {}
                o => return o,
            }
        }
        Ordering::Equal
    }

    pub fn in_range(cur: &str, min: Option<&str>, max: Option<&str>) -> bool {
        let min = min.filter(|s| !s.trim().is_empty());
        let max = max.filter(|s| !s.trim().is_empty());
        if min.is_none() && max.is_none() {
            return true;
        }
        if cur.trim().is_empty() {
            return false;
        }
        min.is_none_or(|m| compare(cur, m) != Ordering::Less)
            && max.is_none_or(|m| compare(m, cur) != Ordering::Less)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn wildcard_ranges() {
            assert!(in_range("1.0.1.80", Some("*.*.1.75"), Some("*.*.1.85")));
            assert!(!in_range("1.0.1.86", Some("*.*.1.75"), Some("*.*.1.85")));
            assert!(in_range("1.0.1.70", None, Some("*.*.1.74")));
            assert!(in_range("2.0.0", None, None));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_table_parses() {
        let c = super::lookup("B171", "B171", "1.0.0.1").unwrap();
        assert_eq!(c.anc_level, Some(63));
        assert!(c.custom_eq.is_some());
        assert!(!c.controls.unwrap().ear_gesture_groups.is_empty());
        assert!(super::lookup("B181", "B181", "1.0.1.80").is_some());
    }

    #[test]
    fn every_sku_has_a_config() {
        for k in crate::models::SKUS {
            for fw in ["1.0.0.10", "1.0.1.80", "2.0.2.99"] {
                assert!(
                    super::lookup(k.alt_base.unwrap_or(k.base), k.base, fw).is_some(),
                    "{} {} fw {fw}",
                    k.id,
                    k.base
                );
            }
        }
    }
}
