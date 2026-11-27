use crate::bt::{self, PairedDevice};
use crate::config::{FirmwareConfig, Gesture, version};
use crate::device::{
    ADVANCED_EQ_DEFAULT_FREQS, Anc, Band, CODEC_NAMES, Device, DeviceState, Phase, SPATIAL_MODES, Strength,
    spatial_modes_for,
};
use crate::models::{self, RingTarget};
use crate::settings::Settings;
use crate::theme::{self, ACCENT, CARD_HI, MUTED, TEXT};
use eframe::egui::{self, Align, Color32, Layout, RichText, Ui, vec2};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

include!(concat!(env!("OUT_DIR"), "/device_images.rs"));

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Sound,
    Controls,
    Device,
}

enum Confirm {
    Codec(u8),
    Dual(bool),
}

pub struct App {
    settings: Settings,
    devices: Vec<PairedDevice>,
    scan: Option<Receiver<std::io::Result<Vec<PairedDevice>>>>,
    scan_error: Option<String>,
    device: Option<Device>,
    last_error: Option<String>,
    remembered: bool,
    auto_connect: bool,
    tab: Tab,
    side: u8,
    confirm: Option<Confirm>,
    toast: Option<(String, Instant)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let settings = Settings::load();
        let mut app = Self {
            settings,
            devices: Vec::new(),
            scan: None,
            scan_error: None,
            device: None,
            last_error: None,
            remembered: false,
            auto_connect: true,
            tab: Tab::Sound,
            side: 2,
            confirm: None,
            toast: None,
        };
        if let Some(sku) = std::env::var("EAR_DEMO").ok().and_then(|id| models::sku_by_id(&id)) {
            app.device = Some(Device::demo(sku));
            app.remembered = true;
            app.auto_connect = false;
        }
        app.rescan(&cc.egui_ctx);
        app
    }

    fn rescan(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(bt::paired_devices());
            ctx.request_repaint();
        });
        self.scan = Some(rx);
    }

    fn connect(&mut self, ctx: &egui::Context, d: PairedDevice) {
        let cached = self.settings.sku_for(d.address).and_then(models::sku_by_id);
        let ctx2 = ctx.clone();
        self.device = Some(Device::connect(d, cached, Arc::new(move || ctx2.request_repaint())));
        self.last_error = None;
        self.remembered = false;
        self.tab = Tab::Sound;
        self.side = 2;
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        if let Some(rx) = &self.scan
            && let Ok(res) = rx.try_recv()
        {
            self.scan = None;
            match res {
                Ok(mut list) => {
                    list.sort_by_key(|d| (!d.connected, !models::looks_like_nothing(&d.name), d.name.to_lowercase()));
                    self.devices = list;
                    self.scan_error = None;
                }
                Err(e) => self.scan_error = Some(e.to_string()),
            }
            if self.auto_connect && self.device.is_none() {
                self.auto_connect = false;
                if let Some(addr) = self.settings.last_address
                    && let Some(d) = self.devices.iter().find(|d| d.address == addr && d.connected).cloned()
                {
                    self.connect(ctx, d);
                }
            }
        }

        let snapshot = self.device.as_ref().map(|d| d.state().clone());
        if let Some(s) = &snapshot {
            match &s.phase {
                Phase::Disconnected(msg) => {
                    self.last_error = (!msg.is_empty()).then(|| msg.clone());
                    self.device = None;
                    self.rescan(ctx);
                }
                Phase::Connected if !self.remembered => {
                    if let Some(sku) = s.sku {
                        self.settings.remember(s.address, sku.id);
                        self.remembered = true;
                    }
                }
                _ => {}
            }
        }

        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG).inner_margin(12)).show(root, |ui| {
            match (self.device.clone(), snapshot) {
                (Some(dev), Some(s)) if s.phase == Phase::Connected => self.main_view(ui, &dev, &s),
                (Some(dev), Some(s)) => self.connecting_view(ui, &dev, &s),
                _ => self.picker_view(ui),
            }
        });

        self.confirm_dialog(ctx);
        if let Some((msg, at)) = &self.toast {
            let left = Duration::from_millis(3200).saturating_sub(at.elapsed());
            if left.is_zero() {
                self.toast = None;
            } else {
                egui::Area::new(egui::Id::unique("toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -24.0))
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(Color32::from_rgb(0x26, 0x26, 0x26))
                            .corner_radius(14)
                            .inner_margin(egui::Margin::symmetric(16, 10))
                            .show(ui, |ui| ui.label(msg.as_str()));
                    });
                ctx.request_repaint_after(left);
            }
        }
    }
}

// ---------------------------------------------------------------- picker

impl App {
    fn picker_view(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(36.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("ear").font(theme::dot(64.0)));
                ui.label(RichText::new("desktop").font(theme::dot(18.0)).color(MUTED));
            });
            ui.add_space(28.0);

            if let Some(e) = &self.last_error {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x2A, 0x10, 0x10))
                    .corner_radius(14)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(e).color(Color32::from_rgb(0xFF, 0xB4, 0xB4)));
                    });
                ui.add_space(8.0);
            }

            let show_all = self.settings.show_all_devices;
            let list: Vec<PairedDevice> =
                self.devices.iter().filter(|d| show_all || models::looks_like_nothing(&d.name)).cloned().collect();
            let mut pick = None;
            theme::card(ui, "Paired devices", |ui| {
                if self.scan.is_some() && self.devices.is_empty() {
                    theme::spinner_text(ui, "Looking for paired devices…");
                } else if let Some(e) = &self.scan_error {
                    ui.label(RichText::new(format!("Bluetooth unavailable: {e}")).color(MUTED));
                } else if list.is_empty() {
                    ui.label(RichText::new("No Nothing / CMF device is paired with this PC.").color(MUTED));
                    ui.label(
                        RichText::new("Pair it in Settings → Bluetooth & devices first, then refresh.")
                            .small()
                            .color(MUTED),
                    );
                }
                for (i, d) in list.iter().enumerate() {
                    if i > 0 {
                        theme::separator(ui);
                    }
                    let resp = ui
                        .horizontal(|ui| {
                            ui.set_min_height(44.0);
                            let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                dot.center(),
                                4.0,
                                if d.connected { ACCENT } else { Color32::from_rgb(0x44, 0x44, 0x44) },
                            );
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&d.name).font(theme::medium(15.0)));
                                let status = if d.connected { "Connected" } else { "Not connected" };
                                ui.label(
                                    RichText::new(format!("{status} · {}", bt::format_address(d.address)))
                                        .small()
                                        .color(MUTED),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new("›").font(theme::medium(22.0)).color(MUTED));
                            });
                        })
                        .response
                        .interact(egui::Sense::click());
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        pick = Some(d.clone());
                    }
                }
            });
            if let Some(d) = pick {
                self.connect(&ctx, d);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::outline_button(ui, "Refresh").clicked() {
                    self.rescan(&ctx);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut v = self.settings.show_all_devices;
                    if theme::toggle(ui, &mut v).changed() {
                        self.settings.show_all_devices = v;
                        self.settings.save();
                    }
                    ui.label(RichText::new("Show all").color(MUTED));
                });
            });
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Unofficial. Not affiliated with Nothing Technology.").small().color(MUTED),
                );
            });
        });
    }

    fn connecting_view(&mut self, ui: &mut Ui, dev: &Device, s: &DeviceState) {
        ui.add_space(ui.available_height() * 0.3);
        ui.vertical_centered(|ui| {
            let name = s.sku.map(|k| k.name).unwrap_or(&s.bt_name);
            ui.label(RichText::new(name).font(theme::dot(30.0)));
            ui.add_space(16.0);
            let text = match &s.phase {
                Phase::Connecting(t) => t.clone(),
                Phase::Rebooting => "Waiting for the device to restart…".into(),
                _ => String::new(),
            };
            theme::spinner_text(ui, &text);
            ui.add_space(20.0);
            if theme::outline_button(ui, "Cancel").clicked() {
                dev.disconnect();
                self.device = None;
                self.auto_connect = false;
            }
        });
    }
}

// ---------------------------------------------------------------- main

fn image(name: &str) -> Option<egui::Image<'static>> {
    let (stem, bytes) = DEVICE_IMAGES.iter().find(|(n, _)| *n == name)?;
    Some(egui::Image::from_bytes(format!("bytes://{stem}.webp"), *bytes))
}

impl App {
    fn main_view(&mut self, ui: &mut Ui, dev: &Device, s: &DeviceState) {
        let spec = s.spec();
        let base = s.base();
        let sku = s.sku.expect("connected implies identified");

        // header
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(sku.name).font(theme::dot(26.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::outline_button(ui, "Disconnect").clicked() {
                    self.settings.last_address = None;
                    self.settings.save();
                    dev.disconnect();
                }
            });
        });

        self.hero(ui, s);
        ui.add_space(6.0);

        let tabs = ["Sound", "Controls", "Device"];
        let cur = match self.tab {
            Tab::Sound => 0,
            Tab::Controls => 1,
            Tab::Device => 2,
        };
        if let Some(i) = theme::segmented(ui, &tabs, Some(cur)) {
            self.tab = [Tab::Sound, Tab::Controls, Tab::Device][i];
        }
        ui.add_space(8.0);

        // separate scroll state per tab
        egui::ScrollArea::vertical().id_salt(cur).auto_shrink([false, false]).show(ui, |ui| {
            match self.tab {
                Tab::Sound => self.sound_tab(ui, dev, s, &spec, base),
                Tab::Controls => self.controls_tab(ui, dev, s, &spec),
                Tab::Device => self.device_tab(ui, dev, s, &spec, base),
            }
            ui.add_space(16.0);
        });
    }

    fn hero(&self, ui: &mut Ui, s: &DeviceState) {
        let sku = s.sku.unwrap();
        let b = s.battery;
        let h = 118.0;
        let single = models::is_headphone(sku.base) || b.single.is_some();
        ui.add_space(6.0);
        if single {
            ui.vertical_centered(|ui| {
                if let Some(img) = image(&format!("{}_left", sku.image)) {
                    ui.add(img.fit_to_exact_size(vec2(h * 1.3, h * 1.3)).maintain_aspect_ratio(true));
                }
                theme::battery(ui, "Battery", b.single.or(b.left).map(|x| x.level), b.single.is_some_and(|x| x.charging));
            });
            return;
        }
        ui.columns(3, |cols| {
            let parts = [("left", "Left", b.left), ("case", "Case", b.case), ("right", "Right", b.right)];
            for (col, (part, label, bat)) in cols.iter_mut().zip(parts) {
                col.vertical_centered(|ui| {
                    let size = if part == "case" { vec2(h, h) } else { vec2(h * 0.78, h) };
                    match image(&format!("{}_{part}", sku.image)) {
                        Some(img) => {
                            let tint = if bat.is_some() { Color32::WHITE } else { Color32::from_white_alpha(90) };
                            ui.add(img.fit_to_exact_size(size).maintain_aspect_ratio(true).tint(tint));
                        }
                        None => {
                            ui.allocate_exact_size(size, egui::Sense::hover());
                        }
                    }
                    theme::battery(ui, label, bat.map(|x| x.level), bat.is_some_and(|x| x.charging));
                });
            }
        });
    }

    // ------------------------------------------------------------ sound

    fn sound_tab(&mut self, ui: &mut Ui, dev: &Device, s: &DeviceState, spec: &FirmwareConfig, base: &str) {
        let anc_bits = spec.anc_level.unwrap_or(0);
        let b157_anc = base != "B157" || s.firmware.as_deref().and_then(|f| f.split('.').nth(2)) == Some("2");
        if anc_bits != 0 && b157_anc {
            theme::card(ui, "Noise control", |ui| {
                let has_tr = anc_bits & 0x10 != 0;
                let mut modes = vec![("Noise cancellation", Anc::On(Strength::High))];
                if has_tr {
                    modes.push(("Transparency", Anc::Transparency));
                }
                modes.push(("Off", Anc::Off));
                let labels: Vec<&str> = modes.iter().map(|m| m.0).collect();
                let sel = s.anc.map(|a| {
                    modes
                        .iter()
                        .position(|m| std::mem::discriminant(&m.1) == std::mem::discriminant(&a))
                        .unwrap_or(0)
                });
                if let Some(i) = theme::segmented(ui, &labels, sel) {
                    let mut target = modes[i].1;
                    if let (Anc::On(_), Some(Anc::On(st))) = (target, s.anc) {
                        target = Anc::On(st);
                    }
                    if target == Anc::Transparency && s.flat_eq {
                        self.toast("Transparency is unavailable while Flat EQ is on");
                    } else {
                        dev.set_anc(target);
                    }
                }
                if let Some(Anc::On(st)) = s.anc
                    && anc_bits & 0x04 != 0
                {
                    let mut levels = vec![("High", Strength::High)];
                    if anc_bits & 0x02 != 0 {
                        levels.push(("Mid", Strength::Mid));
                    }
                    levels.push(("Low", Strength::Low));
                    if anc_bits & 0x08 != 0 {
                        levels.push(("Adaptive", Strength::Adaptive));
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("Strength").small().color(MUTED));
                    let labels: Vec<&str> = levels.iter().map(|l| l.0).collect();
                    let sel = levels.iter().position(|l| l.1 == st);
                    if let Some(i) = theme::segmented(ui, &labels, sel) {
                        dev.set_anc(Anc::On(levels[i].1));
                    }
                }
            });
            ui.add_space(8.0);
        }

        theme::card(ui, "Equalizer", |ui| {
            if s.flat_eq {
                ui.label(RichText::new("Flat EQ is on — slide the switch on the left earcup to change it.").color(MUTED));
                return;
            }
            let presets = models::presets(base);
            let w = (ui.available_width() - 8.0) / 2.0;
            for pair in presets.chunks(2) {
                ui.horizontal(|ui| {
                    for p in pair {
                        if theme::pill(ui, p.label, s.eq == Some(p.wire), w).clicked() && s.eq != Some(p.wire) {
                            self.select_preset(dev, s, spec, base, p.wire);
                        }
                    }
                });
            }
            let custom = models::custom_preset(base);
            if custom.is_some() && s.eq == custom {
                ui.add_space(6.0);
                let mut v = s.custom_eq.unwrap_or([0.0; 3]);
                let mut changed = false;
                for (i, name) in ["Bass", "Mid", "Treble"].iter().enumerate() {
                    theme::row(ui, name, None, |ui| {
                        ui.label(RichText::new(format!("{:+.0} dB", v[i])).color(MUTED));
                        let r = ui.add(egui::Slider::new(&mut v[i], -6.0..=6.0).step_by(1.0).show_value(false));
                        changed |= r.changed();
                    });
                }
                if changed {
                    dev.set_custom_eq(v);
                }
            }
            if models::has_advanced_preset(base) && s.eq == Some(models::PRESET_ADVANCED) {
                ui.add_space(6.0);
                self.advanced_eq(ui, dev, s);
            }
        });
        ui.add_space(8.0);

        if models::has_bass_enhance(base) {
            theme::card(ui, "Bass enhance", |ui| {
                let (on, lvl) = s.bass.unwrap_or((false, 0));
                if let Some(v) = theme::toggle_row(ui, "Bass enhance", None, s.bass.map(|b| b.0)) {
                    if v && spec.mutually_exclusive != 0 && s.spatial.is_some_and(|x| x != 0) {
                        self.toast("Bass enhance isn't available while Spatial audio is on");
                    } else {
                        dev.set_bass(v, lvl);
                    }
                }
                if on {
                    let n = models::bass_levels(base).len();
                    let labels: Vec<String> = if n == 3 {
                        vec!["Low".into(), "Mid".into(), "High".into()]
                    } else {
                        (1..=n).map(|i| i.to_string()).collect()
                    };
                    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                    if let Some(i) = theme::segmented(ui, &labels, Some(lvl)) {
                        dev.set_bass(true, i);
                    }
                }
            });
            ui.add_space(8.0);
        }

        let spatial = spec.spatial_audio != 0 && spec.use_audiodo_spatial == 0;
        let personal = spec.mimi != 0 || spec.audiodo != 0;
        if spatial || personal {
            theme::card(ui, "Audio", |ui| {
                if spatial {
                    ui.label("Spatial audio");
                    let modes = spatial_modes_for(spec.spatial_audio);
                    let cols = modes.len().min(3);
                    let w = (ui.available_width() - 8.0 * (cols as f32 - 1.0)) / cols as f32;
                    for chunk in modes.chunks(3) {
                        ui.horizontal(|ui| {
                            for &i in chunk {
                                let n = SPATIAL_MODES[i].name;
                                if theme::pill(ui, n, s.spatial == Some(i), w).clicked() {
                                    let eq_busy = s.advanced_eq_on || s.bass.is_some_and(|b| b.0);
                                    if i != 0 && spec.mutually_exclusive != 0 && eq_busy {
                                        self.toast("Turn off Advanced EQ and Bass enhance to use Spatial audio");
                                    } else {
                                        dev.set_spatial(i);
                                    }
                                }
                            }
                        });
                    }
                }
                if personal {
                    if spatial {
                        theme::separator(ui);
                    }
                    if let Some(v) = theme::toggle_row(
                        ui,
                        "Personal sound",
                        Some("Uses the hearing profile stored on the device"),
                        s.personal_sound,
                    ) {
                        dev.set_personal_sound(v);
                    }
                }
            });
        }
    }

    fn select_preset(&mut self, dev: &Device, s: &DeviceState, spec: &FirmwareConfig, base: &str, wire: u8) {
        let lm = models::uses_listening_mode(base);
        if lm && wire == 0 && spec.dirac_opteo_support == 1 && s.codec == Some(2) {
            return self.toast("Dirac Opteo isn't available while LDAC is on");
        }
        if !lm && wire == models::PRESET_ADVANCED && spec.mutually_exclusive != 0 && s.spatial.is_some_and(|x| x != 0) {
            return self.toast("Advanced EQ isn't available while Spatial audio is on");
        }
        if s.long_power == Some(true) && base == "B189" {
            return self.toast("EQ is locked while Long battery mode is on");
        }
        dev.set_preset(wire);
    }

    fn advanced_eq(&mut self, ui: &mut Ui, dev: &Device, s: &DeviceState) {
        let mut bands = if s.advanced_bands.is_empty() {
            ADVANCED_EQ_DEFAULT_FREQS.iter().map(|&f| Band { filter: 1, gain: 0.0, freq: f, q: 1.0 }).collect()
        } else {
            s.advanced_bands.clone()
        };
        let n = bands.len().max(1);
        let w = ui.available_width() / n as f32;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().slider_width = 150.0;
            for b in bands.iter_mut() {
                ui.allocate_ui_with_layout(vec2(w, 210.0), Layout::top_down(Align::Center), |ui| {
                    ui.label(RichText::new(format!("{:+.1}", b.gain)).small().color(MUTED));
                    changed |= ui
                        .add(egui::Slider::new(&mut b.gain, -6.0..=6.0).step_by(0.5).vertical().show_value(false))
                        .changed();
                    let f = if b.freq >= 1000.0 { format!("{:.1}k", b.freq / 1000.0) } else { format!("{:.0}", b.freq) };
                    ui.label(RichText::new(f).small());
                });
            }
        });
        ui.horizontal(|ui| {
            if theme::outline_button(ui, "Reset").clicked() {
                bands.iter_mut().for_each(|b| b.gain = 0.0);
                changed = true;
            }
        });
        if changed {
            dev.set_advanced_bands(bands);
        }
    }

    // ------------------------------------------------------------ controls

    fn controls_tab(&mut self, ui: &mut Ui, dev: &Device, s: &DeviceState, spec: &FirmwareConfig) {
        let Some(controls) = &spec.controls else {
            theme::card(ui, "", |ui| {
                theme::spinner_text(ui, "Reading gesture layout…");
            });
            return;
        };
        let fw = s.firmware.clone().unwrap_or_default();
        // sides present in the config
        let mut sides: Vec<(&str, u8)> = Vec::new();
        let types: Vec<i32> =
            controls.ear_gesture_groups.iter().flat_map(|g| g.gestures.iter().map(|x| x.device_type)).collect();
        if types.contains(&2) {
            sides.push(("Left", 2));
            sides.push(("Right", 3));
        }
        if types.contains(&6) {
            sides.push(("Device", 6));
        }
        if types.contains(&4) {
            sides.push(("Case", 4));
        }
        if !sides.iter().any(|x| x.1 == self.side) {
            self.side = sides.first().map(|x| x.1).unwrap_or(2);
        }
        if sides.len() > 1 {
            let labels: Vec<&str> = sides.iter().map(|x| x.0).collect();
            if let Some(i) = theme::segmented(ui, &labels, sides.iter().position(|x| x.1 == self.side)) {
                self.side = sides[i].1;
            }
            ui.add_space(8.0);
        }
        let code = self.side;
        let cfg_type = if code == 3 { 2 } else { code as i32 };

        let mut any = false;
        for group in &controls.ear_gesture_groups {
            let gestures: Vec<&Gesture> = group.gestures.iter().filter(|g| g.device_type == cfg_type).collect();
            if gestures.is_empty() {
                continue;
            }
            any = true;
            let fixed_group = group.group_name.as_deref().is_some_and(|n| n.starts_with("gesture_not_customisation"));
            let title = match group.group_name.as_deref() {
                None | Some("") => "Gestures".to_string(),
                Some(n) if n.starts_with("gesture_not_customisation") => "Fixed gestures".to_string(),
                Some(n) => models::label(n),
            };
            theme::card(ui, &title, |ui| {
                for (i, g) in gestures.iter().enumerate() {
                    if i > 0 {
                        theme::separator(ui);
                    }
                    self.gesture_row(ui, dev, s, g, code, &fw, fixed_group);
                }
            });
            ui.add_space(8.0);
        }
        if !any {
            theme::card(ui, "", |ui| {
                ui.label(RichText::new("No customisable controls on this side.").color(MUTED));
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gesture_row(&mut self, ui: &mut Ui, dev: &Device, s: &DeviceState, g: &Gesture, code: u8, fw: &str, fixed: bool) {
        let name = models::label(&g.gesture_name_key);
        let current = s
            .gestures
            .iter()
            .find(|r| r.device == code && r.button as i32 == g.button && r.gesture as i32 == g.gesture)
            .map(|r| r.action);
        let ops: Vec<(i32, String)> = g
            .operation_groups
            .iter()
            .filter(|og| !og.group_name.starts_with("gesture_not_customisation"))
            .flat_map(|og| og.operations.iter())
            .filter(|o| o.is_custom && o.support_platform.is_none() && o.operation >= 0)
            .filter(|o| o.limit_version.as_deref().is_none_or(|v| fw.is_empty() || version::compare(fw, v).is_ge()))
            .map(|o| (o.operation, o.operation_desc_key.iter().map(|k| models::label(k)).collect::<Vec<_>>().join(" / ")))
            .collect();
        let fixed_desc: Vec<String> = g
            .operation_groups
            .iter()
            .filter(|og| og.group_name.starts_with("gesture_not_customisation") || fixed || !g.is_custom)
            .flat_map(|og| og.operations.iter())
            .map(|o| o.operation_desc_key.iter().map(|k| models::label(k)).collect::<Vec<_>>().join(" / "))
            .collect();

        if fixed || !g.is_custom || ops.is_empty() || g.gesture < 0 {
            theme::row(ui, &name, None, |ui| {
                ui.label(RichText::new(fixed_desc.join(", ")).color(MUTED));
            });
            return;
        }
        let shown = current.map(|a| if (20..=22).contains(&a) { 10 } else { a });
        let cur_label = shown
            .and_then(|a| ops.iter().find(|o| o.0 == a).map(|o| o.1.clone()))
            .unwrap_or_else(|| if current.is_some() { "Other".into() } else { "—".into() });
        let both = g.gesture_name_key == "pinch_both_buds";
        let targets: Vec<u8> = if both && (code == 2 || code == 3) { vec![2, 3] } else { vec![code] };
        let mut pick = None;
        theme::row(ui, &name, None, |ui| {
            egui::ComboBox::from_id_salt(("gesture", code, g.button, g.gesture))
                .selected_text(cur_label)
                .width(170.0)
                .show_ui(ui, |ui| {
                    for (op, label) in &ops {
                        if ui.selectable_label(shown == Some(*op), label).clicked() {
                            pick = Some(*op);
                        }
                    }
                });
        });
        if let Some(op) = pick {
            for t in &targets {
                dev.set_gesture(*t, g.button as u8, g.gesture as u8, op);
            }
        }
        if shown == Some(10) {
            // which noise-control modes this gesture cycles through: [transparency, anc, off]
            let mut sel = match current {
                Some(20) => [false, true, true],
                Some(21) => [true, false, true],
                Some(22) => [true, true, false],
                _ => [true, true, true],
            };
            let before = sel;
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("Cycles").small().color(MUTED));
                ui.checkbox(&mut sel[0], "Transparency");
                ui.checkbox(&mut sel[1], "Noise cancellation");
                ui.checkbox(&mut sel[2], "Off");
            });
            if sel != before {
                let action = match sel {
                    [true, true, true] => Some(10),
                    [false, true, true] => Some(20),
                    [true, false, true] => Some(21),
                    [true, true, false] => Some(22),
                    _ => None,
                };
                match action {
                    Some(a) => {
                        for t in &targets {
                            dev.set_gesture(*t, g.button as u8, g.gesture as u8, a);
                        }
                    }
                    None => self.toast("Pick at least two modes to cycle through"),
                }
            }
        }
    }

    // ------------------------------------------------------------ device

    fn device_tab(&mut self, ui: &mut Ui, dev: &Device, s: &DeviceState, spec: &FirmwareConfig, base: &str) {
        theme::card(ui, "Find my device", |ui| {
            ui.label(RichText::new("Plays a loud tone. Take the earbuds out of your ears first.").small().color(MUTED));
            ui.add_space(2.0);
            let ringing = |pos: u8| s.ringing.iter().any(|r| r.0 == pos && r.1);
            let targets: Vec<(&str, u8)> = match models::ring_target(base) {
                RingTarget::Buds => vec![("left", 2), ("right", 3)],
                RingTarget::Body | RingTarget::Single => vec![("", 6)],
            };
            let w = (ui.available_width() - 8.0) / targets.len() as f32;
            ui.horizontal(|ui| {
                for (label, pos) in targets {
                    let on = ringing(pos);
                    let text = match (on, label.is_empty()) {
                        (true, true) => "Stop".to_string(),
                        (true, false) => format!("Stop {label}"),
                        (false, true) => "Ring".to_string(),
                        (false, false) => format!("Ring {label}"),
                    };
                    let (rect, resp) = ui.allocate_exact_size(vec2(w, 38.0), egui::Sense::click());
                    let fill = if on { ACCENT } else if resp.hovered() { Color32::from_rgb(0x30, 0x30, 0x30) } else { CARD_HI };
                    ui.painter().rect_filled(rect, 255, fill);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(13.5), TEXT);
                    if resp.clicked() {
                        dev.ring(pos, !on);
                    }
                }
            });
        });
        ui.add_space(8.0);

        if models::has_ear_fit_test(base) || spec.ear_tip_fit_test != 0 {
            theme::card(ui, "Ear tip fit test", |ui| {
                let verdict = |v: u8| match v {
                    0 => "Good seal",
                    1 => "Adjust or try another tip size",
                    _ => "Not detected",
                };
                theme::row(ui, "Check the seal of each earbud", None, |ui| {
                    if s.ear_fit_started.is_some_and(|t| t.elapsed() < Duration::from_secs(20)) {
                        ui.ctx().request_repaint_after(Duration::from_secs(1));
                        ui.add(egui::Spinner::new().color(TEXT));
                    } else if theme::outline_button(ui, "Start test").clicked() {
                        dev.start_ear_fit_test();
                    }
                });
                if let Some((l, r)) = s.ear_fit {
                    ui.label(RichText::new(format!("Left: {}", verdict(l))).color(if l == 0 { TEXT } else { ACCENT }));
                    ui.label(RichText::new(format!("Right: {}", verdict(r))).color(if r == 0 { TEXT } else { ACCENT }));
                }
            });
            ui.add_space(8.0);
        }

        theme::card(ui, "Settings", |ui| {
            let mut first = true;
            let mut sep = |ui: &mut Ui| {
                if !first {
                    theme::separator(ui);
                }
                first = false;
            };
            if models::has_in_ear(base) {
                sep(ui);
                let hint = if models::is_headphone(base) { "Pause when taken off" } else { "Pause when removed from ears" };
                if let Some(v) = theme::toggle_row(ui, "In-ear detection", Some(hint), s.in_ear) {
                    dev.set_in_ear(v);
                }
            }
            sep(ui);
            if let Some(v) = theme::toggle_row(ui, "Low lag mode", Some("Lower latency for games and video"), s.low_latency) {
                dev.set_low_latency(v);
            }
            if models::has_personalized_anc(base) {
                sep(ui);
                if let Some(v) = theme::toggle_row(ui, "Personalised ANC", None, s.personal_anc) {
                    dev.set_personal_anc(v);
                }
            }
            if spec.long_power_mode != 0 {
                sep(ui);
                if let Some(v) = theme::toggle_row(ui, "Long battery life mode", Some("Forces AAC and disables spatial audio and bass boost"), s.long_power) {
                    dev.set_long_power(v);
                }
            }
            if spec.call_transparency != 0 {
                sep(ui);
                if let Some(v) = theme::toggle_row(ui, "Call transparency", Some("Hear your own voice on calls"), s.call_transparency) {
                    dev.set_call_transparency(v);
                }
            }
            if spec.super_mic != 0 {
                sep(ui);
                if let Some(v) = theme::toggle_row(ui, "Super Mic", None, s.super_mic) {
                    dev.set_super_mic(v);
                }
                if s.mic_mode.is_some() {
                    let opts = ["Natural", "Voice focus"];
                    if let Some(i) = theme::segmented(ui, &opts, s.mic_mode.map(|m| m as usize)) {
                        dev.set_mic_mode(i as u8);
                    }
                }
            }
            if spec.walkie_talkie_mode != 0 {
                sep(ui);
                if let Some(v) = theme::toggle_row(ui, "Walkie-talkie mode", None, s.walkie_talkie) {
                    dev.set_walkie_talkie(v);
                }
            }
            if spec.support_leakage_protection != 0 {
                sep(ui);
                ui.label("Anti-leakage");
                let opts = ["Off", "Calls only", "Audio + calls"];
                if let Some(i) = theme::segmented(ui, &opts, s.anti_leakage.map(|m| m as usize)) {
                    dev.set_anti_leakage(i as u8);
                }
            }
        });
        ui.add_space(8.0);

        if spec.high_quality_audio != 0 {
            theme::card(ui, "Audio codec", |ui| {
                let mut idx = vec![0u8];
                if spec.high_quality_audio & 2 != 0 {
                    idx.push(1);
                }
                if spec.high_quality_audio & 4 != 0 {
                    idx.push(2);
                }
                let labels: Vec<&str> = idx.iter().map(|&i| CODEC_NAMES[i as usize]).collect();
                let sel = s.codec.and_then(|c| idx.iter().position(|&i| i == c));
                if let Some(i) = theme::segmented(ui, &labels, sel) {
                    if s.long_power == Some(true) {
                        self.toast("Codec is locked to AAC in Long battery life mode");
                    } else if Some(i) != sel {
                        self.confirm = Some(Confirm::Codec(idx[i]));
                    }
                }
                ui.label(RichText::new("Switching codec restarts the device.").small().color(MUTED));
            });
            ui.add_space(8.0);
        }

        if spec.dual_connection != 0 {
            theme::card(ui, "Dual connection", |ui| {
                if let Some(v) = theme::toggle_row(ui, "Connect to two devices", None, s.dual_enabled) {
                    if spec.dual_connection_reboot != 0 {
                        self.confirm = Some(Confirm::Dual(v));
                    } else {
                        dev.set_dual_enabled(v);
                    }
                }
                if s.dual_enabled == Some(true) {
                    for d in &s.dual_list {
                        theme::separator(ui);
                        let name = if d.name.is_empty() { "Unknown device".to_string() } else { d.name.clone() };
                        let hint = if d.is_self { "This PC" } else if d.connected { "Connected" } else { "Paired" };
                        theme::row(ui, &name, Some(hint), |ui| {
                            if !d.is_self {
                                let label = if d.connected { "Disconnect" } else { "Connect" };
                                if theme::outline_button(ui, label).clicked() {
                                    dev.dual_connect(d.mac, !d.connected);
                                }
                            }
                        });
                    }
                }
            });
            ui.add_space(8.0);
        }

        theme::card(ui, "About", |ui| {
            let sku = s.sku.unwrap();
            let info = [
                ("Model", sku.name.to_string()),
                ("Model id", format!("{} · {}", sku.base, sku.id)),
                ("Firmware", s.firmware.clone().unwrap_or_else(|| "…".into())),
                ("Bluetooth address", bt::format_address(s.address)),
                ("Bluetooth name", s.bt_name.clone()),
            ];
            for (k, v) in info {
                theme::row(ui, k, None, |ui| {
                    ui.label(RichText::new(v).color(MUTED));
                });
            }
        });
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let Some(dev) = self.device.clone() else {
            self.confirm = None;
            return;
        };
        let (title, body) = match c {
            Confirm::Codec(2) => (
                "Switch to LDAC?",
                "LDAC may reduce battery life and can cause choppy audio with interference. The device will restart.",
            ),
            Confirm::Codec(_) => ("Switch codec?", "The device will restart to apply the new codec."),
            Confirm::Dual(_) => ("Change dual connection?", "The device will restart to apply this change."),
        };
        let mut close = false;
        let mut apply = false;
        egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.label(RichText::new(title).font(theme::medium(17.0)));
            ui.add_space(4.0);
            ui.label(RichText::new(body).color(MUTED));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Restart").color(Color32::BLACK)).fill(TEXT)).clicked() {
                    apply = true;
                }
                if theme::outline_button(ui, "Cancel").clicked() {
                    close = true;
                }
            });
        });
        if apply {
            match *c {
                Confirm::Codec(i) => dev.set_codec(i),
                Confirm::Dual(v) => dev.set_dual_enabled(v),
            }
            close = true;
        }
        if close {
            self.confirm = None;
        }
    }
}
