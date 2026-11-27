//! Product table and the per-model quirks that ear-web hard-codes in its device pages.

pub struct Sku {
    pub id: &'static str,
    pub name: &'static str,
    pub base: &'static str,
    pub alt_base: Option<&'static str>,
    /// Image prefix in `assets/devices`, e.g. `b171_white` (+ `_left|_right|_case.webp`).
    pub image: &'static str,
}

include!("skus.rs");

pub fn sku_by_id(id: &str) -> Option<&'static Sku> {
    SKUS.iter().find(|s| s.id.eq_ignore_ascii_case(id))
}

/// Fallback when the Fast Pair model id can't be read: match the Bluetooth name.
pub fn sku_by_name(bt_name: &str) -> Option<&'static Sku> {
    let norm = |s: &str| {
        s.to_ascii_lowercase()
            .replace("nothing", "")
            .replace("cmf", "")
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let target = norm(bt_name);
    if target.is_empty() {
        return None;
    }
    // rev(): max_by_key keeps the last maximum, so this yields the first-listed SKU
    SKUS.iter()
        .rev()
        .filter(|s| {
            let n = norm(s.name);
            !n.is_empty() && target.contains(&n)
        })
        .max_by_key(|s| norm(s.name).len())
}

pub fn looks_like_nothing(bt_name: &str) -> bool {
    let n = bt_name.to_ascii_lowercase();
    // Nothing / CMF phones share the brand but have no earbud control channel
    if n.contains("nothing phone") || n.contains("cmf phone") {
        return false;
    }
    n.contains("nothing") || n.contains("cmf") || n.starts_with("ear") || sku_by_name(bt_name).is_some()
}

pub fn is_headphone(base: &str) -> bool {
    matches!(base, "B170" | "B175" | "B186" | "B192" | "B164")
}

fn any(base: &str, list: &[&str]) -> bool {
    list.contains(&base)
}

/// Models whose presets are "listening modes" (Dirac / POP / Rock …) on 0xF01D.
pub fn uses_listening_mode(base: &str) -> bool {
    any(base, &["B172", "B195", "B168", "B179", "B197", "B184", "B185", "B175", "B189"])
}

pub fn has_bass_enhance(base: &str) -> bool {
    any(
        base,
        &["B171", "B172", "B195", "B168", "B162", "B184", "B179", "B197", "B170", "B185", "B164", "B173", "B189", "B186"],
    )
}

/// Bass enhance UI positions -> wire byte.
pub fn bass_levels(base: &str) -> &'static [u8] {
    if any(base, &["B189", "B186", "B195", "B197"]) { &[0, 5, 10] } else { &[2, 4, 6, 8, 10] }
}

pub fn has_ear_fit_test(base: &str) -> bool {
    any(base, &["B155", "B171", "B172", "B195", "B162", "B184", "B179", "B197", "B173", "B190"])
}

pub fn has_in_ear(base: &str) -> bool {
    !any(base, &["B174", "B185", "B175", "B189", "B186", "B164"])
}

pub fn has_personalized_anc(base: &str) -> bool {
    base == "B155"
}

/// Models whose advanced-EQ toggle doubles as the "Advanced" preset (index 6).
pub fn advanced_eq_is_preset(base: &str) -> bool {
    any(base, &["B157", "B155", "B171", "B174", "B170", "B186", "B192", "B173", "B190"])
}

pub enum RingTarget {
    /// Ear (1): one command rings both buds.
    Single,
    /// Headphones / neckband: one body, device code 6.
    Body,
    /// Separate left (2) / right (3) buds.
    Buds,
}

pub fn ring_target(base: &str) -> RingTarget {
    match base {
        "B181" => RingTarget::Single,
        "B170" | "B164" | "B175" | "B186" | "B192" => RingTarget::Body,
        _ => RingTarget::Buds,
    }
}

#[derive(Clone, Copy)]
pub struct Preset {
    pub wire: u8,
    pub label: &'static str,
}

const fn p(wire: u8, label: &'static str) -> Preset {
    Preset { wire, label }
}

pub const PRESET_CUSTOM_EQ: u8 = 5;
pub const PRESET_CUSTOM_LM: u8 = 6;
pub const PRESET_ADVANCED: u8 = 6;

/// Preset buttons in display order, as laid out by each ear-web device page.
pub fn presets(base: &str) -> &'static [Preset] {
    const ONE: &[Preset] = &[p(0, "Balanced"), p(3, "More Bass"), p(2, "More Treble"), p(1, "Voice")];
    const BASIC: &[Preset] = &[p(0, "Balanced"), p(3, "More Bass"), p(2, "More Treble"), p(1, "Voice"), p(5, "Custom")];
    const ADV: &[Preset] =
        &[p(0, "Balanced"), p(3, "More Bass"), p(2, "More Treble"), p(1, "Voice"), p(5, "Custom"), p(6, "Advanced")];
    const LANTURN: &[Preset] = &[
        p(0, "Balanced"),
        p(3, "More Bass"),
        p(2, "More Treble"),
        p(1, "More Vocals"),
        p(7, "More Detail"),
        p(5, "Custom"),
        p(6, "Advanced"),
    ];
    const DIRAC: &[Preset] = &[
        p(0, "Dirac Opteo"),
        p(3, "Pop"),
        p(1, "Rock"),
        p(5, "Classical"),
        p(2, "Electronic"),
        p(4, "Enhance Vocals"),
        p(6, "Custom"),
    ];
    const DIRAC7: &[Preset] = &[
        p(7, "Dirac"),
        p(3, "Pop"),
        p(1, "Rock"),
        p(5, "Classical"),
        p(2, "Electronic"),
        p(4, "Enhance Vocals"),
        p(6, "Custom"),
    ];
    const CLIP: &[Preset] =
        &[p(3, "Pop"), p(1, "Rock"), p(2, "Electronic"), p(5, "Classical"), p(4, "Enhance Vocals"), p(6, "Custom")];
    match base {
        "B181" => ONE,
        "B163" | "B162" | "B164" => BASIC,
        "B192" => LANTURN,
        "B185" => DIRAC7,
        "B189" => CLIP,
        b if uses_listening_mode(b) => DIRAC,
        _ => ADV,
    }
}

pub fn custom_preset(base: &str) -> Option<u8> {
    let w = if uses_listening_mode(base) { PRESET_CUSTOM_LM } else { PRESET_CUSTOM_EQ };
    presets(base).iter().any(|p| p.wire == w).then_some(w)
}

pub fn has_advanced_preset(base: &str) -> bool {
    !uses_listening_mode(base) && presets(base).iter().any(|p| p.wire == PRESET_ADVANCED)
}

/// English strings for the i18n keys used in the gesture config.
pub fn label(key: &str) -> String {
    let s = match key {
        "control_single_press" | "single_press" => "Single press",
        "control_double_press" | "double_press" => "Double press",
        "control_triple_press" | "triple_press" => "Triple press",
        "control_press_hold" | "press_hold" | "Hold" | "hold" => "Press and hold",
        "control_double_press_hold" | "double_press_hold" => "Double press and hold",
        "control_double_tap" => "Double tap",
        "control_triple_tap" => "Triple tap",
        "control_tap_hold" => "Tap and hold",
        "control_double_tap_hold" | "double_tap_hole" => "Double tap and hold",
        "control_slide_on_system" | "control_slider" => "Slide",
        "pinch_both_buds" => "Pinch both buds",
        "roll" => "Roll",
        "rotate" => "Rotate",
        "paddle_r_l" => "Push paddle",
        "paddle_hold_r_l" => "Push and hold paddle",
        "toggle_left_right" => "Toggle",
        "control_no_action" => "No action",
        "control_no_extra_action" => "No extra action",
        "control_play_pause" => "Play / pause",
        "control_answer_call" => "Answer call",
        "control_answer_hang_up" => "Answer / hang up",
        "control_decline_incoming_call" | "decline_incoming_calls" => "Decline incoming call",
        "control_hand_up_decline_incoming_calls" => "Hang up / decline call",
        "control_skip_back" => "Skip back",
        "control_skip_forward" => "Skip forward",
        "control_skip_forward_back" => "Skip forward / back",
        "anc_noise_control" => "Noise control",
        "control_voice_assistant" => "Voice assistant",
        "control_volume_up" => "Volume up",
        "control_volume_down" => "Volume down",
        "control_volume_control" => "Volume control",
        "control_mic_mute" => "Mute mic",
        "control_mic_on_off" => "Mic on / off",
        "low_latency_mode" | "case_game_model" => "Low latency mode",
        "camera_shutter" => "Camera shutter",
        "case_answer_call_mute" => "Answer call / mute",
        "ear_spatial_audio" => "Spatial audio",
        "essential_space" => "Essential Space",
        "eq_preset" => "Cycle EQ preset",
        "eq_switch_on" => "EQ on / off",
        "control_operation_ultra_bass" => "Bass enhance",
        "control_operation_treble_enhance" => "Treble enhance",
        "recording" => "Recording",
        "pairing_mode" => "Pairing mode",
        "fast_forward" => "Fast forward",
        "switch_bluetooth_connection" => "Switch Bluetooth connection",
        "gesture_latch_supermic" => "Super Mic",
        "talk_button_title" => "Talk",
        "control_button" => "Control button",
        "scroll_wheel" | "control_group_scroll_wheel" => "Roller",
        "paddle" => "Paddle",
        "magic_button" => "Action button",
        "guide_slider_title" => "Slider",
        "eq_toggle" => "EQ toggle",
        "gesture_not_customisation" | "gesture_not_customisation_function_button" => "Fixed",
        other => {
            let t = other.trim_start_matches("control_").replace('_', " ");
            let mut c = t.chars();
            return match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            };
        }
    };
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_fallback() {
        assert_eq!(sku_by_name("Nothing Ear (2)").unwrap().base, "B155");
        assert_eq!(sku_by_name("CMF Buds Pro 2").unwrap().base, "B172");
        assert_eq!(sku_by_name("Nothing Headphone (1)").unwrap().base, "B170");
        assert!(sku_by_name("Logitech Mouse").is_none());
        assert!(!looks_like_nothing("Nothing Phone (2)"));
        assert!(looks_like_nothing("Nothing Headphone (1) Pro"));
    }
}
