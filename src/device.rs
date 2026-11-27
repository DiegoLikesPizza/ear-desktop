//! One connection to one device: a worker thread owns the socket, serialises
//! requests (each waits for its reply or a timeout), decodes replies and pushes
//! into a shared [`DeviceState`] that the UI renders from.

use crate::bt::{self, Rfcomm};
use crate::config::{self, FirmwareConfig};
use crate::models::{self, Sku};
use crate::protocol::{self, Frame, Reassembler, cmd};
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const GET_TIMEOUT: Duration = Duration::from_millis(450);
const SET_TIMEOUT: Duration = Duration::from_millis(2000);
/// Writes where only the latest value matters (sliders, mode pickers).
const SUPERSEDABLE: &[u16] = &[
    cmd::SET_CUSTOM_EQ,
    cmd::SET_ADVANCED_EQ_VALUE,
    cmd::SET_BASS,
    cmd::SET_ANC,
    cmd::SET_EQ,
    cmd::SET_LISTENING_MODE,
    cmd::SET_SPATIAL,
];

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Connecting(String),
    Connected,
    Rebooting,
    Disconnected(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bat {
    pub level: u8,
    pub charging: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Battery {
    pub left: Option<Bat>,
    pub right: Option<Bat>,
    pub case: Option<Bat>,
    pub single: Option<Bat>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
    High,
    Mid,
    Low,
    Adaptive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anc {
    Off,
    Transparency,
    On(Strength),
}

impl Anc {
    fn wire(self) -> u8 {
        match self {
            Anc::Off => 0x05,
            Anc::Transparency => 0x07,
            Anc::On(Strength::High) => 0x01,
            Anc::On(Strength::Mid) => 0x02,
            Anc::On(Strength::Low) => 0x03,
            Anc::On(Strength::Adaptive) => 0x04,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub filter: u8,
    pub gain: f32,
    pub freq: f32,
    pub q: f32,
}

pub const ADVANCED_EQ_DEFAULT_FREQS: [f32; 8] = [55.0, 110.0, 220.0, 440.0, 1320.0, 3300.0, 6600.0, 13200.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureRecord {
    pub device: u8,
    pub button: u8,
    pub gesture: u8,
    pub action: i32,
}

#[derive(Clone, Debug)]
pub struct DualDevice {
    pub mac: [u8; 6],
    pub name: String,
    pub is_self: bool,
    pub connected: bool,
}

pub struct SpatialMode {
    pub name: &'static str,
    /// capability bit in the config's `spatialAudio` mask (0 = always available)
    pub bit: u32,
    mode: u8,
    head: u8,
}

pub const SPATIAL_MODES: &[SpatialMode] = &[
    SpatialMode { name: "Off", bit: 0, mode: 0, head: 0 },
    SpatialMode { name: "Head tracked", bit: 0x1, mode: 1, head: 1 },
    SpatialMode { name: "Fixed", bit: 0x2, mode: 1, head: 0 },
    SpatialMode { name: "Concert", bit: 0x8, mode: 2, head: 0 },
    SpatialMode { name: "Theatre", bit: 0x10, mode: 3, head: 0 },
    SpatialMode { name: "Game", bit: 0x20, mode: 4, head: 0 },
];

/// Indexes into [`SPATIAL_MODES`] that a model's `spatialAudio` mask allows.
pub fn spatial_modes_for(mask: u32) -> Vec<usize> {
    (0..SPATIAL_MODES.len()).filter(|&i| SPATIAL_MODES[i].bit == 0 || mask & SPATIAL_MODES[i].bit != 0).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn spatial_mask() {
        // Ear (3a): spatialAudio = 6 -> Off + Fixed only
        let names: Vec<_> = super::spatial_modes_for(6).iter().map(|&i| super::SPATIAL_MODES[i].name).collect();
        assert_eq!(names, ["Off", "Fixed"]);
        // Headphone (a): 28 -> Off, Concert, Theatre
        assert_eq!(super::spatial_modes_for(28), [0, 3, 4]);
    }
}

pub const CODEC_NAMES: [&str; 3] = ["AAC", "LHDC", "LDAC"];

#[derive(Clone, Default)]
pub struct DeviceState {
    pub phase: Phase,
    pub address: u64,
    pub bt_name: String,
    pub sku: Option<&'static Sku>,
    pub firmware: Option<String>,
    pub specs: Option<Arc<FirmwareConfig>>,
    pub battery: Battery,
    pub anc: Option<Anc>,
    pub eq: Option<u8>,
    pub advanced_eq_on: bool,
    /// bass, mid, treble in dB
    pub custom_eq: Option<[f32; 3]>,
    pub advanced_bands: Vec<Band>,
    /// (enabled, ui level index)
    pub bass: Option<(bool, usize)>,
    pub in_ear: Option<bool>,
    pub low_latency: Option<bool>,
    pub personal_anc: Option<bool>,
    pub gestures: Vec<GestureRecord>,
    pub dual_enabled: Option<bool>,
    pub dual_list: Vec<DualDevice>,
    pub codec: Option<u8>,
    pub spatial: Option<usize>,
    pub personal_sound: Option<bool>,
    pub super_mic: Option<bool>,
    pub mic_mode: Option<u8>,
    pub call_transparency: Option<bool>,
    pub walkie_talkie: Option<bool>,
    pub long_power: Option<bool>,
    pub anti_leakage: Option<u8>,
    pub flat_eq: bool,
    /// position (2 left, 3 right, 6 body) -> ringing
    pub ringing: Vec<(u8, bool)>,
    pub ear_fit: Option<(u8, u8)>,
    pub ear_fit_started: Option<Instant>,
    pub expect_reboot: bool,
}

impl DeviceState {
    pub fn base(&self) -> &'static str {
        self.sku.map(|s| s.base).unwrap_or("")
    }
    pub fn spec(&self) -> Arc<FirmwareConfig> {
        self.specs.clone().unwrap_or_default()
    }
}

enum Event {
    Frame(Frame),
    Send { command: u16, payload: Vec<u8> },
    Closed(u32),
    Disconnect,
}

/// Cheap, cloneable handle used by the UI.
#[derive(Clone)]
pub struct Device {
    state: Arc<Mutex<DeviceState>>,
    tx: Sender<Event>,
}

/// Where the UI learns a session ended (to remember the SKU, etc.)
pub type Repaint = Arc<dyn Fn() + Send + Sync>;

impl Device {
    pub fn connect(target: bt::PairedDevice, cached_sku: Option<&'static Sku>, repaint: Repaint) -> Self {
        let state = Arc::new(Mutex::new(DeviceState {
            phase: Phase::Connecting("Connecting…".into()),
            address: target.address,
            bt_name: target.name.clone(),
            sku: cached_sku,
            ..Default::default()
        }));
        let (tx, rx) = mpsc::channel();
        let dev = Device { state: state.clone(), tx: tx.clone() };
        std::thread::Builder::new()
            .name("device-session".into())
            .spawn(move || Session { state, tx, repaint, op: 0, generation: 0 }.run(rx, target))
            .expect("spawn session thread");
        dev
    }

    /// Offline stand-in with plausible state, for UI work without hardware (`EAR_DEMO=<sku id>`).
    pub fn demo(sku: &'static Sku) -> Self {
        let fw = "1.0.2.50".to_string();
        let specs = config::lookup(sku.alt_base.unwrap_or(sku.base), sku.base, &fw).map(Arc::new);
        let headphone = models::is_headphone(sku.base);
        let bat = |level, charging| Some(Bat { level, charging });
        let state = DeviceState {
            phase: Phase::Connected,
            address: 0x2C_BE_EB_12_34_56,
            bt_name: sku.name.to_string(),
            sku: Some(sku),
            firmware: Some(fw),
            battery: if headphone {
                Battery { single: bat(82, false), ..Default::default() }
            } else {
                Battery { left: bat(87, false), right: bat(64, false), case: bat(45, true), single: None }
            },
            anc: Some(Anc::On(Strength::High)),
            eq: models::presets(sku.base).first().map(|p| p.wire),
            custom_eq: Some([2.0, 0.0, -1.0]),
            bass: Some((true, 1)),
            in_ear: Some(true),
            low_latency: Some(false),
            personal_anc: Some(false),
            dual_enabled: Some(true),
            dual_list: vec![
                DualDevice { mac: [1, 2, 3, 4, 5, 6], name: "This PC".into(), is_self: true, connected: true },
                DualDevice { mac: [6, 5, 4, 3, 2, 1], name: "Phone (3)".into(), is_self: false, connected: false },
            ],
            codec: Some(0),
            spatial: Some(0),
            personal_sound: Some(false),
            super_mic: Some(false),
            mic_mode: Some(0),
            call_transparency: Some(false),
            walkie_talkie: Some(false),
            long_power: Some(false),
            anti_leakage: Some(0),
            specs,
            ..Default::default()
        };
        let (tx, _rx) = mpsc::channel();
        Device { state: Arc::new(Mutex::new(state)), tx }
    }

    pub fn state(&self) -> MutexGuard<'_, DeviceState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn disconnect(&self) {
        let _ = self.tx.send(Event::Disconnect);
    }

    fn send(&self, command: u16, payload: Vec<u8>) {
        let _ = self.tx.send(Event::Send { command, payload });
    }

    // ---- actions (optimistic: update state, then write) ----

    pub fn set_anc(&self, anc: Anc) {
        self.state().anc = Some(anc);
        self.send(cmd::SET_ANC, vec![0x01, anc.wire(), 0x00]);
    }

    pub fn set_preset(&self, wire: u8) {
        let base = {
            let mut s = self.state();
            s.eq = Some(wire);
            s.base()
        };
        if models::uses_listening_mode(base) {
            self.send(cmd::SET_LISTENING_MODE, vec![wire, 0]);
        } else if models::has_advanced_preset(base) {
            if wire == models::PRESET_ADVANCED {
                self.state().advanced_eq_on = true;
                self.send(cmd::SET_ADVANCED_EQ, vec![1, 0]);
                self.send(cmd::GET_ADVANCED_EQ_VALUE, vec![255]);
                return;
            }
            self.state().advanced_eq_on = false;
            self.send(cmd::SET_ADVANCED_EQ, vec![0, 0]);
            self.send(cmd::SET_EQ, vec![wire, 0]);
        } else {
            self.send(cmd::SET_EQ, vec![wire, 0]);
        }
        if Some(wire) == models::custom_preset(base) {
            self.send(cmd::GET_CUSTOM_EQ, vec![]);
        }
    }

    /// `values` = [bass, mid, treble] in dB.
    pub fn set_custom_eq(&self, values: [f32; 3]) {
        let spec = {
            let mut s = self.state();
            s.custom_eq = Some(values);
            s.spec().custom_eq
        };
        let Some(c) = spec else { return };
        // device band order: peak (mid), high shelf (treble), low shelf (bass)
        let bands = [
            Band { filter: 1, gain: values[1], freq: c.freq_peak, q: c.q_peak },
            Band { filter: 2, gain: values[2], freq: c.freq_high, q: c.q_high },
            Band { filter: 0, gain: values[0], freq: c.freq_low, q: c.q_low },
        ];
        let total = -bands.iter().fold(0.0f32, |m, b| m.max(b.gain));
        let mut p = vec![bands.len() as u8];
        p.extend(protocol::f32_le(total));
        for b in &bands {
            push_band(&mut p, b);
        }
        p.extend(std::iter::repeat_n(0u8, bands.len() * 3));
        self.send(cmd::SET_CUSTOM_EQ, p);
    }

    pub fn set_advanced_bands(&self, bands: Vec<Band>) {
        let total = -bands.iter().fold(0.0f32, |m, b| m.max(b.gain));
        let mut p = vec![0u8, bands.len() as u8];
        p.extend(protocol::f32_le(total));
        for b in &bands {
            push_band(&mut p, &Band { filter: 1, ..*b });
        }
        self.state().advanced_bands = bands;
        self.send(cmd::SET_ADVANCED_EQ_VALUE, p);
    }

    pub fn set_bass(&self, enabled: bool, level: usize) {
        let base = {
            let mut s = self.state();
            s.bass = Some((enabled, level));
            s.base()
        };
        let wire = models::bass_levels(base).get(level).copied().unwrap_or(0);
        self.send(cmd::SET_BASS, vec![enabled as u8, wire]);
    }

    pub fn set_in_ear(&self, on: bool) {
        self.state().in_ear = Some(on);
        self.send(cmd::SET_IN_EAR, vec![0x01, 0x01, on as u8]);
    }

    pub fn set_low_latency(&self, on: bool) {
        self.state().low_latency = Some(on);
        self.send(cmd::SET_LATENCY, vec![if on { 1 } else { 2 }, 0]);
    }

    pub fn set_personal_anc(&self, on: bool) {
        self.state().personal_anc = Some(on);
        self.send(cmd::SET_PERSONAL_ANC, vec![on as u8]);
    }

    pub fn set_gesture(&self, device: u8, button: u8, gesture: u8, action: i32) {
        {
            let mut s = self.state();
            match s.gestures.iter_mut().find(|g| g.device == device && g.button == button && g.gesture == gesture) {
                Some(g) => g.action = action,
                None => s.gestures.push(GestureRecord { device, button, gesture, action }),
            }
        }
        self.send(cmd::SET_GESTURE, vec![0x01, device, button, gesture, action as u8]);
    }

    /// `position`: 2 left, 3 right, 6 body / both.
    pub fn ring(&self, position: u8, on: bool) {
        let base = {
            let mut s = self.state();
            match s.ringing.iter_mut().find(|r| r.0 == position) {
                Some(r) => r.1 = on,
                None => s.ringing.push((position, on)),
            }
            s.base()
        };
        let payload = match models::ring_target(base) {
            models::RingTarget::Single => vec![on as u8],
            models::RingTarget::Body => vec![0x06, on as u8],
            models::RingTarget::Buds => vec![position, on as u8],
        };
        self.send(cmd::SET_RING, payload);
    }

    pub fn start_ear_fit_test(&self) {
        {
            let mut s = self.state();
            s.ear_fit = None;
            s.ear_fit_started = Some(Instant::now());
        }
        self.send(cmd::SET_EAR_FIT_TEST, vec![0x01]);
    }

    pub fn set_dual_enabled(&self, on: bool) {
        let reboot = {
            let mut s = self.state();
            s.dual_enabled = Some(on);
            let r = s.spec().dual_connection_reboot != 0;
            s.expect_reboot |= r;
            r
        };
        self.send(cmd::SET_DUAL_ENABLE, vec![on as u8]);
        if on && !reboot {
            self.send(cmd::GET_DUAL_LIST, vec![0]);
        }
    }

    pub fn dual_connect(&self, mac: [u8; 6], connect: bool) {
        let mut p = vec![connect as u8];
        p.extend(mac);
        self.send(cmd::SET_DUAL_CONNECT, p);
    }

    pub fn set_codec(&self, index: u8) {
        {
            let mut s = self.state();
            s.codec = Some(index);
            s.expect_reboot = true;
        }
        self.send(cmd::SET_CODEC, vec![index]);
    }

    pub fn set_spatial(&self, index: usize) {
        self.state().spatial = Some(index);
        let m = &SPATIAL_MODES[index];
        self.send(cmd::SET_SPATIAL, vec![m.mode, m.head]);
    }

    pub fn set_personal_sound(&self, on: bool) {
        let spec = {
            let mut s = self.state();
            s.personal_sound = Some(on);
            s.spec()
        };
        if spec.mimi != 0 {
            self.send(cmd::SET_MIMI, vec![on as u8]);
        } else if spec.audiodo != 0 {
            self.send(cmd::SET_AUDIODO, vec![on as u8]);
        }
    }

    pub fn set_super_mic(&self, on: bool) {
        self.state().super_mic = Some(on);
        self.send(cmd::SET_SUPER_MIC, vec![on as u8]);
    }
    pub fn set_mic_mode(&self, mode: u8) {
        self.state().mic_mode = Some(mode);
        self.send(cmd::SET_MIC_MODE, vec![mode]);
    }
    pub fn set_call_transparency(&self, on: bool) {
        self.state().call_transparency = Some(on);
        self.send(cmd::SET_CALL_TRANSPARENCY, vec![on as u8]);
    }
    pub fn set_walkie_talkie(&self, on: bool) {
        self.state().walkie_talkie = Some(on);
        self.send(cmd::SET_WALKIE_TALKIE, vec![on as u8]);
    }
    pub fn set_long_power(&self, on: bool) {
        self.state().long_power = Some(on);
        self.send(cmd::SET_LONG_POWER, vec![on as u8]);
    }
    pub fn set_anti_leakage(&self, mode: u8) {
        self.state().anti_leakage = Some(mode);
        self.send(cmd::SET_ANTI_LEAKAGE, vec![mode]);
    }

}

fn push_band(p: &mut Vec<u8>, b: &Band) {
    p.push(b.filter);
    p.extend(protocol::f32_le(b.gain));
    p.extend(protocol::f32_le(b.freq));
    p.extend(protocol::f32_le(b.q));
}

struct Pending {
    command: u16,
    payload: Vec<u8>,
}

struct Inflight {
    command: u16,
    op: u8,
    deadline: Instant,
}

struct Session {
    state: Arc<Mutex<DeviceState>>,
    tx: Sender<Event>,
    repaint: Repaint,
    op: u8,
    generation: u32,
}

enum End {
    User,
    Lost(String),
}

impl Session {
    fn lock(&self) -> MutexGuard<'_, DeviceState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn set_phase(&self, phase: Phase) {
        self.lock().phase = phase;
        (self.repaint)();
    }

    fn run(mut self, rx: Receiver<Event>, target: bt::PairedDevice) {
        // Identify the product first (cached across launches by the UI).
        if self.lock().sku.is_none() {
            self.set_phase(Phase::Connecting("Identifying model…".into()));
            let sku = bt::read_fastpair_model(target.address)
                .ok()
                .and_then(|id| models::sku_by_id(&id))
                .or_else(|| models::sku_by_name(&target.name));
            self.lock().sku = sku;
        }
        if self.lock().sku.is_none() {
            self.set_phase(Phase::Disconnected(format!("“{}” isn't a supported Nothing / CMF device", target.name)));
            return;
        }

        let mut attempt = 0;
        loop {
            if !self.lock().expect_reboot {
                self.set_phase(Phase::Connecting("Opening control channel…".into()));
            }
            let sock = match Rfcomm::connect(target.address, bt::SPP_UUID) {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    let rebooting = self.lock().expect_reboot;
                    // the SPP channel can refuse briefly right after the Fast Pair probe closes
                    if !rebooting && attempt < 2 {
                        attempt += 1;
                        if wait_or_quit(&rx, Duration::from_millis(700)) {
                            return self.set_phase(Phase::Disconnected(String::new()));
                        }
                        continue;
                    }
                    if rebooting && attempt < 8 {
                        attempt += 1;
                        self.set_phase(Phase::Rebooting);
                        if wait_or_quit(&rx, Duration::from_secs(4)) {
                            return self.set_phase(Phase::Disconnected(String::new()));
                        }
                        continue;
                    }
                    return self.set_phase(Phase::Disconnected(friendly(&e)));
                }
            };
            attempt = 0;
            {
                let mut s = self.lock();
                s.expect_reboot = false;
            }
            self.set_phase(Phase::Connected);
            match self.serve(&rx, sock) {
                End::User => return self.set_phase(Phase::Disconnected(String::new())),
                End::Lost(why) => {
                    if self.lock().expect_reboot {
                        self.set_phase(Phase::Rebooting);
                        if wait_or_quit(&rx, Duration::from_secs(8)) {
                            return self.set_phase(Phase::Disconnected(String::new()));
                        }
                        continue;
                    }
                    return self.set_phase(Phase::Disconnected(why));
                }
            }
        }
    }

    fn serve(&mut self, rx: &Receiver<Event>, sock: Arc<Rfcomm>) -> End {
        // reader thread: socket -> frames -> session channel
        self.generation += 1;
        let generation = self.generation;
        {
            let sock = sock.clone();
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let mut r = Reassembler::default();
                let mut buf = [0u8; 2048];
                loop {
                    match sock.read(&mut buf) {
                        Ok(0) | Err(_) => {
                            let _ = tx.send(Event::Closed(generation));
                            return;
                        }
                        Ok(n) => r.push(&buf[..n], |f| {
                            let _ = tx.send(Event::Frame(f));
                        }),
                    }
                }
            });
        }

        let mut queue: VecDeque<Pending> = VecDeque::new();
        let base = self.lock().base();
        for (c, p) in initial_requests(base) {
            queue.push_back(Pending { command: c, payload: p });
        }
        let mut inflight: Option<Inflight> = None;
        let mut dual_pages = 0u32;

        let end = loop {
            // pump
            if inflight.is_none()
                && let Some(next) = queue.pop_front()
            {
                self.op = if self.op >= 255 { 1 } else { self.op + 1 };
                let frame = protocol::encode(next.command, self.op, &next.payload);
                if let Err(e) = sock.write_all(&frame) {
                    break End::Lost(friendly(&e));
                }
                let timeout = if protocol::is_set(next.command) { SET_TIMEOUT } else { GET_TIMEOUT };
                inflight = Some(Inflight { command: next.command, op: self.op, deadline: Instant::now() + timeout });
            }

            let wait = inflight
                .as_ref()
                .map(|i| i.deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(3600));
            match rx.recv_timeout(wait) {
                Ok(Event::Frame(f)) => {
                    if let Some(i) = &inflight
                        && protocol::is_reply_to(f.command, i.command)
                        && (!protocol::is_set(i.command) || f.op == i.op)
                    {
                        inflight = None;
                    }
                    let follow = self.handle(&f, &mut dual_pages);
                    for (c, p) in follow {
                        queue.push_back(Pending { command: c, payload: p });
                    }
                    (self.repaint)();
                }
                Ok(Event::Send { command, payload }) => {
                    // a newer write of the same setting supersedes a queued one (e.g. slider drags)
                    if SUPERSEDABLE.contains(&command) {
                        queue.retain(|q| q.command != command);
                    }
                    queue.push_back(Pending { command, payload });
                }
                Ok(Event::Closed(g)) if g == generation => break End::Lost("Connection lost".into()),
                Ok(Event::Closed(_)) => {}
                Ok(Event::Disconnect) => break End::User,
                Err(RecvTimeoutError::Timeout) => inflight = None,
                Err(RecvTimeoutError::Disconnected) => break End::User,
            }
        };
        sock.shutdown();
        end
    }

    /// Decode one frame into state; returns follow-up requests.
    fn handle(&mut self, f: &Frame, dual_pages: &mut u32) -> Vec<(u16, Vec<u8>)> {
        let p = &f.payload[..];
        let key = f.command | 0x8000;
        let mut follow = Vec::new();
        let mut s = self.lock();
        let base = s.base();
        let flag = |p: &[u8]| p.first() == Some(&1);
        match key {
            cmd::GET_BATTERY | cmd::PUSH_BATTERY => {
                let mut b = Battery::default();
                let n = *p.first().unwrap_or(&0) as usize;
                for i in 0..n {
                    let (Some(&id), Some(&v)) = (p.get(1 + i * 2), p.get(2 + i * 2)) else { break };
                    let bat = Some(Bat { level: v & 0x7F, charging: v & 0x80 != 0 });
                    match id {
                        2 => b.left = bat,
                        3 => b.right = bat,
                        4 => b.case = bat,
                        6 => b.single = bat,
                        _ => {}
                    }
                }
                s.battery = b;
            }
            cmd::GET_ANC | cmd::PUSH_ANC => {
                let mut anc = s.anc;
                for e in p.chunks(3) {
                    if e.len() < 2 {
                        continue;
                    }
                    let st = match e[1] {
                        1 => Some(Strength::High),
                        2 => Some(Strength::Mid),
                        3 => Some(Strength::Low),
                        4 => Some(Strength::Adaptive),
                        _ => None,
                    };
                    match (e[0], e[1]) {
                        (1, 0 | 5) => anc = Some(Anc::Off),
                        (1, 7 | 0xFE) => anc = Some(Anc::Transparency),
                        (1, _) if st.is_some() => anc = st.map(Anc::On),
                        (2, _) if st.is_some() => {
                            if let Some(Anc::On(_)) = anc {
                                anc = st.map(Anc::On);
                            }
                        }
                        _ => {}
                    }
                }
                s.anc = anc;
            }
            cmd::GET_EQ | cmd::GET_LISTENING_MODE => {
                if let Some(&m) = p.first()
                    && !s.advanced_eq_on
                {
                    s.eq = Some(m);
                }
            }
            cmd::GET_ADVANCED_EQ => {
                s.advanced_eq_on = flag(p);
                if s.advanced_eq_on && models::advanced_eq_is_preset(base) {
                    s.eq = Some(models::PRESET_ADVANCED);
                }
            }
            cmd::GET_ADVANCED_EQ_VALUE => {
                if p.len() >= 6 {
                    let n = p[1] as usize;
                    s.advanced_bands = p[6..]
                        .chunks_exact(13)
                        .take(n)
                        .map(|c| Band {
                            filter: c[0],
                            gain: protocol::f32_from_wire(&c[1..5]),
                            freq: protocol::f32_from_wire(&c[5..9]),
                            q: protocol::f32_from_wire(&c[9..13]),
                        })
                        .collect();
                }
            }
            cmd::GET_CUSTOM_EQ => {
                let g = |i: usize| p.get(6 + i * 13..10 + i * 13).map(protocol::f32_from_wire);
                if let (Some(peak), Some(high), Some(low)) = (g(0), g(1), g(2)) {
                    s.custom_eq = Some([low.round(), peak.round(), high.round()]);
                }
            }
            cmd::GET_BASS => {
                if p.len() >= 2 {
                    let lv = models::bass_levels(base).iter().position(|&l| l == p[1]).unwrap_or(0);
                    s.bass = Some((p[0] == 1, lv));
                }
            }
            cmd::GET_IN_EAR => {
                if let Some(&v) = p.get(2) {
                    s.in_ear = Some(v == 1);
                }
            }
            cmd::GET_LATENCY => s.low_latency = p.first().map(|&v| v == 1),
            cmd::GET_PERSONAL_ANC => s.personal_anc = Some(flag(p)),
            cmd::GET_GESTURES => {
                let n = *p.first().unwrap_or(&0) as usize;
                s.gestures = p[1.min(p.len())..]
                    .chunks_exact(4)
                    .take(n)
                    .map(|c| GestureRecord { device: c[0], button: c[1], gesture: c[2], action: c[3] as i32 })
                    .collect();
            }
            cmd::GET_FIRMWARE => {
                let fw = String::from_utf8_lossy(p).trim_matches(char::from(0)).trim().to_string();
                let sku = s.sku;
                let specs = sku.and_then(|k| config::lookup(k.alt_base.unwrap_or(k.base), k.base, &fw)).map(Arc::new);
                let first = s.firmware.is_none() || s.specs.is_none();
                s.firmware = Some(fw.clone());
                s.specs = specs.clone();
                if first {
                    follow = feature_requests(base, &fw, specs.as_deref());
                }
            }
            cmd::GET_DUAL_ENABLE => s.dual_enabled = Some(flag(p)),
            cmd::GET_DUAL_LIST => {
                if p.len() >= 3 {
                    let count = p[2] as usize;
                    let mut off = 3;
                    let mut added = false;
                    for _ in 0..count {
                        if off + 8 > p.len() {
                            break;
                        }
                        let flags = p[off];
                        let mut mac = [0u8; 6];
                        mac.copy_from_slice(&p[off + 1..off + 7]);
                        let nl = (p[off + 7] & 0x7F) as usize;
                        let name = String::from_utf8_lossy(p.get(off + 8..off + 8 + nl).unwrap_or(&[])).to_string();
                        let d = DualDevice { mac, name, is_self: flags & 0xF0 != 0, connected: flags & 0x0F != 0 };
                        match s.dual_list.iter_mut().find(|x| x.mac == mac) {
                            Some(x) => *x = d,
                            None => {
                                s.dual_list.push(d);
                                added = true;
                            }
                        }
                        off += 8 + nl;
                    }
                    if added && *dual_pages < 20 {
                        *dual_pages += 1;
                        follow.push((cmd::GET_DUAL_LIST, vec![s.dual_list.len() as u8]));
                    }
                }
            }
            cmd::PUSH_DUAL_EVENT => {
                if p.len() >= 8 {
                    let mut mac = [0u8; 6];
                    mac.copy_from_slice(&p[1..7]);
                    match s.dual_list.iter_mut().find(|x| x.mac == mac) {
                        Some(x) => x.connected = p[0] == 1,
                        None => {
                            *dual_pages = 0;
                            follow.push((cmd::GET_DUAL_LIST, vec![s.dual_list.len() as u8]));
                        }
                    }
                }
            }
            cmd::GET_CODEC => s.codec = Some(p.first().copied().filter(|&i| i < 3).unwrap_or(0)),
            cmd::GET_SPATIAL => {
                let (mode, head) = (p.first().copied().unwrap_or(0), p.get(1).copied().unwrap_or(0));
                s.spatial = Some(if head == 1 {
                    1
                } else {
                    SPATIAL_MODES.iter().position(|m| m.head == 0 && m.mode == mode).unwrap_or(0)
                });
            }
            cmd::GET_MIMI | cmd::GET_AUDIODO => s.personal_sound = Some(flag(p)),
            cmd::PUSH_AUDIODO => {
                if s.spec().audiodo != 0 {
                    s.personal_sound = Some(flag(p));
                }
            }
            cmd::GET_SUPER_MIC => s.super_mic = Some(flag(p)),
            cmd::GET_MIC_MODE => s.mic_mode = p.first().copied(),
            cmd::GET_CALL_TRANSPARENCY => s.call_transparency = Some(flag(p)),
            cmd::GET_WALKIE_TALKIE => s.walkie_talkie = Some(flag(p)),
            cmd::GET_LONG_POWER => s.long_power = Some(flag(p)),
            cmd::GET_ANTI_LEAKAGE => s.anti_leakage = p.first().copied(),
            cmd::GET_FLAT_EQ | cmd::PUSH_FLAT_EQ => s.flat_eq = flag(p),
            cmd::PUSH_EAR_FIT => {
                if p.len() >= 2 {
                    s.ear_fit = Some((p[0], p[1]));
                    s.ear_fit_started = None;
                }
            }
            cmd::PUSH_FIND_STATE => {
                if !p.is_empty() && p.len() == 1 + 2 * p[0] as usize {
                    s.ringing = p[1..].chunks_exact(2).map(|c| (c[0], c[1] == 1)).collect();
                }
            }
            _ => {}
        }
        follow
    }
}

fn initial_requests(base: &str) -> Vec<(u16, Vec<u8>)> {
    let mut r = vec![(cmd::GET_BATTERY, vec![]), (cmd::GET_FIRMWARE, vec![])];
    r.push(if models::uses_listening_mode(base) { (cmd::GET_LISTENING_MODE, vec![]) } else { (cmd::GET_EQ, vec![]) });
    if base != "B157" {
        r.push((cmd::GET_ANC, vec![]));
    }
    if base != "B181" {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        r.push((cmd::SET_UTC_TIME, (t as u32).to_be_bytes().to_vec()));
    }
    if models::has_in_ear(base) {
        r.push((cmd::GET_IN_EAR, vec![]));
    }
    r.push((cmd::GET_LATENCY, vec![]));
    if models::has_personalized_anc(base) {
        r.push((cmd::GET_PERSONAL_ANC, vec![]));
    }
    r.push((cmd::GET_GESTURES, vec![]));
    r.push((cmd::GET_ADVANCED_EQ, vec![]));
    if models::has_bass_enhance(base) {
        r.push((cmd::GET_BASS, vec![]));
    }
    r
}

/// Requests that depend on the firmware's capability block.
fn feature_requests(base: &str, fw: &str, spec: Option<&FirmwareConfig>) -> Vec<(u16, Vec<u8>)> {
    let mut r = Vec::new();
    if base == "B157" && fw.split('.').nth(2) == Some("2") {
        r.push((cmd::GET_ANC, vec![]));
    }
    if models::custom_preset(base).is_some() && base != "B181" {
        r.push((cmd::GET_CUSTOM_EQ, vec![]));
    }
    if models::has_advanced_preset(base) {
        r.push((cmd::GET_ADVANCED_EQ_VALUE, vec![255]));
    }
    let Some(c) = spec else { return r };
    if c.dual_connection != 0 {
        r.push((cmd::GET_DUAL_ENABLE, vec![]));
        r.push((cmd::GET_DUAL_LIST, vec![0]));
    }
    if c.high_quality_audio != 0 {
        r.push((cmd::GET_CODEC, vec![]));
    }
    if c.spatial_audio != 0 && c.use_audiodo_spatial == 0 {
        r.push((cmd::GET_SPATIAL, vec![]));
    }
    if c.mimi != 0 {
        r.push((cmd::GET_MIMI, vec![]));
    } else if c.audiodo != 0 {
        r.push((cmd::GET_AUDIODO, vec![]));
    }
    if c.super_mic != 0 {
        r.push((cmd::GET_SUPER_MIC, vec![]));
        r.push((cmd::GET_MIC_MODE, vec![]));
    }
    if c.call_transparency != 0 {
        r.push((cmd::GET_CALL_TRANSPARENCY, vec![]));
    }
    if c.walkie_talkie_mode != 0 {
        r.push((cmd::GET_WALKIE_TALKIE, vec![]));
    }
    if c.long_power_mode != 0 {
        r.push((cmd::GET_LONG_POWER, vec![]));
    }
    if c.support_leakage_protection != 0 {
        r.push((cmd::GET_ANTI_LEAKAGE, vec![]));
    }
    if c.support_studio != 0 {
        r.push((cmd::GET_FLAT_EQ, vec![]));
    }
    r
}

/// Sleep, but return true early if the user asked to disconnect.
fn wait_or_quit(rx: &Receiver<Event>, d: Duration) -> bool {
    let end = Instant::now() + d;
    loop {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(Event::Disconnect) | Err(RecvTimeoutError::Disconnected) => return true,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => return false,
        }
    }
}

fn friendly(e: &std::io::Error) -> String {
    match e.raw_os_error() {
        Some(10060) => "Timed out — is the device on and nearby?".into(),
        Some(10050) | Some(10051) => "Bluetooth is off or no adapter was found".into(),
        Some(10061) => "Device refused the connection — close other apps using it".into(),
        Some(10048) => "The control channel is busy (another app is connected to it)".into(),
        Some(10108) => "Service not found on this device".into(),
        _ => e.to_string(),
    }
}
