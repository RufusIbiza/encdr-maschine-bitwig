use std::net::UdpSocket;
use crossbeam_channel::Sender;
use rosc::{OscPacket, OscType, OscMessage, encoder};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::sync::Arc;
use encdr::{Encdr, DeviceId, LedValue, Event};
use std::time::{Duration, Instant};

use crate::nav;


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavDir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageMode {
    Device,
    Mixer,
    Channel,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackData {
    pub index: usize,
    pub name: String,
    pub color: String,
    pub volume: f32,
    pub volume_display: String,
    pub pan: f32,
    pub pan_display: String,
    pub meter: f32,
    pub muted: bool,
    pub solo: bool,
    pub arm: bool,
    pub exists: bool,
    /// Deactivated tracks are hidden from the strip and skipped by navigation, as in Bitwig.
    pub activated: bool,
    pub selected: bool,
    pub is_group: bool,
    pub is_group_expanded: bool,
    /// Nesting depth in the track hierarchy (0 = top level).
    pub depth: u32,
    pub has_parent: bool,
    pub parent_color: String,
    pub parent_name: String,
    pub has_grandparent: bool,
    pub grandparent_color: String,
    pub grandparent_name: String,
}

impl Default for TrackData {
    fn default() -> Self {
        Self {
            index: 0,
            name: "Track".to_string(),
            color: "#ff8c1a".to_string(),
            volume: 0.8,
            volume_display: "0.0 dB".to_string(),
            pan: 0.5,
            pan_display: "C".to_string(),
            meter: 0.0,
            muted: false,
            solo: false,
            arm: false,
            exists: false,
            activated: true,
            selected: false,
            is_group: false,
            is_group_expanded: false,
            depth: 0,
            has_parent: false,
            parent_color: String::new(),
            parent_name: String::new(),
            has_grandparent: false,
            grandparent_color: String::new(),
            grandparent_name: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendData {
    pub index: usize,
    pub name: String,
    pub value: f32,
    pub display: String,
}

impl Default for SendData {
    fn default() -> Self {
        Self {
            index: 0,
            name: "Send".to_string(),
            value: 0.0,
            display: "-inf dB".to_string(),
        }
    }
}

pub struct LogicState {
    pub page_mode: PageMode,
    // Phase 1 Device State
    pub device_name: String,
    pub param_names: [String; 8],
    pub param_displays: [String; 8],
    pub param_values: [f32; 8],
    pub param_modulated: [f32; 8],
    pub track_muted: bool,

    // Phase 2 Mixer & Bank State
    pub tracks: Vec<TrackData>,
    pub bank_offset: i32,
    pub bank_can_scroll_back: bool,
    pub bank_can_scroll_forw: bool,

    /// The whole (visible) track list from the script's large strip bank, for the
    /// channel strip and 4D-encoder navigation. Only name/colour/hierarchy fields are filled.
    pub strip: Vec<TrackData>,
    pub strip_selected_idx: usize,
    pub strip_offset: i32,
    pub strip_can_scroll_back: bool,
    pub strip_can_scroll_forw: bool,

    // Phase 2 Channel State
    pub selected_track_idx: usize,
    pub channel_name: String,
    pub channel_color: String,
    pub channel_volume: f32,
    pub channel_volume_display: String,
    pub channel_pan: f32,
    pub channel_pan_display: String,
    pub channel_meter: f32,
    pub channel_meter_l: f32,
    pub channel_meter_r: f32,
    pub channel_max_peak: f32,
    pub channel_muted: bool,
    pub channel_solo: bool,
    pub channel_arm: bool,
    pub channel_is_group: bool,
    pub channel_is_group_expanded: bool,
    pub channel_is_master: bool,
    pub channel_monitor_mode: String,
    pub master_name: String,
    pub master_color: String,
    pub sends: Vec<SendData>,
    pub sends_page: usize,
    pub sends_total: usize,

    // Hardware Input Modifiers
    pub shift_pressed: bool,
    pub mute_held: bool,
    pub solo_held: bool,
    pub dirty: bool,

    /// Group we just asked Bitwig to expand; its first child gets selected once it appears in the bank.
    pub pending_enter: Option<PendingEnter>,
    /// Last colours written to the compass LEDs (up, left, right, down), to avoid resending unchanged values.
    pub compass_leds: Option<[LedValue; 4]>,
    /// Finger resting on the 4D encoder (shows the strip's scroll indicator).
    pub nav_touched: bool,
    /// Bumped on every 4D-encoder navigation so the screens can flash the scroll indicator.
    pub nav_seq: u64,
}

pub struct PendingEnter {
    pub group_idx: usize,
    pub group_name: String,
    pub since: Instant,
}

impl Default for LogicState {
    fn default() -> Self {
        let mut tracks = Vec::with_capacity(16);
        for i in 0..16 {
            tracks.push(TrackData {
                index: i,
                name: format!("Track {}", i + 1),
                color: "#ff8c1a".to_string(),
                volume: 0.8,
                volume_display: "0.0 dB".to_string(),
                pan: 0.5,
                pan_display: "C".to_string(),
                meter: 0.0,
                muted: false,
                solo: false,
                arm: false,
                exists: false,
                activated: true,
                selected: i == 0,
                is_group: false,
                is_group_expanded: false,
                depth: 0,
                has_parent: false,
                parent_color: String::new(),
                parent_name: String::new(),
                has_grandparent: false,
                grandparent_color: String::new(),
                grandparent_name: String::new(),
            });
        }

        let mut sends = Vec::with_capacity(12);
        for s in 0..12 {
            sends.push(SendData {
                index: s,
                name: format!("Send {}", ((b'A' + s as u8) as char)),
                value: 0.0,
                display: "-inf dB".to_string(),
            });
        }

        Self {
            page_mode: PageMode::Mixer,
            device_name: String::new(),
            param_names: Default::default(),
            param_displays: Default::default(),
            param_values: [0.0; 8],
            param_modulated: [0.0; 8],
            track_muted: false,

            tracks,
            bank_offset: 0,
            bank_can_scroll_back: false,
            bank_can_scroll_forw: false,
            strip: Vec::new(),
            strip_selected_idx: 0,
            strip_offset: 0,
            strip_can_scroll_back: false,
            strip_can_scroll_forw: false,

            selected_track_idx: 0,
            channel_name: "No Track".to_string(),
            channel_color: "#ff8c1a".to_string(),
            channel_volume: 0.8,
            channel_volume_display: "0.0 dB".to_string(),
            channel_pan: 0.5,
            channel_pan_display: "C".to_string(),
            channel_meter: 0.0,
            channel_meter_l: 0.0,
            channel_meter_r: 0.0,
            channel_max_peak: 0.0,
            channel_muted: false,
            channel_solo: false,
            channel_arm: false,
            channel_is_group: false,
            channel_is_group_expanded: false,
            channel_is_master: false,
            channel_monitor_mode: "AUTO".to_string(),
            master_name: "Master".to_string(),
            master_color: "#4a90e2".to_string(),
            sends,
            sends_page: 0,
            sends_total: 6,

            shift_pressed: false,
            mute_held: false,
            solo_held: false,
            dirty: true,

            pending_enter: None,
            compass_leds: None,
            nav_touched: false,
            nav_seq: 0,
        }
    }
}

pub struct AppLogic {
    pub state: LogicState,
    osc_sender: UdpSocket,
    state_tx: Sender<Value>,
    encdr: Arc<Encdr>,
    device_id: DeviceId,
    encoder_names: [&'static str; 8],
    top_button_names: [&'static str; 8],
    modifier_used_as_gesture: bool,
}

impl AppLogic {
    pub fn new(state_tx: Sender<Value>, encdr: Arc<Encdr>, device_id: DeviceId) -> Self {
        let osc_sender = UdpSocket::bind("0.0.0.0:0").unwrap();
        let mut app = Self {
            state: LogicState::default(),
            osc_sender,
            state_tx,
            encdr,
            device_id,
            encoder_names: [
                "screen_encoder_1", "screen_encoder_2", "screen_encoder_3", "screen_encoder_4",
                "screen_encoder_5", "screen_encoder_6", "screen_encoder_7", "screen_encoder_8",
            ],
            top_button_names: [
                "top_1", "top_2", "top_3", "top_4",
                "top_5", "top_6", "top_7", "top_8",
            ],
            modifier_used_as_gesture: false,
        };

        app.set_page_mode(PageMode::Mixer);
        app.check_and_send_state();

        // Broadcast refresh on initialization so host pushes full state
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.send_osc("/refresh".to_string(), vec![]);
        app
    }

    pub fn set_page_mode(&mut self, mode: PageMode) {
        self.state.page_mode = mode;
        self.state.dirty = true;

        let plugin_led = if mode == PageMode::Device { LedValue::Bright } else { LedValue::Dim };
        let mixer_led = if mode == PageMode::Mixer { LedValue::Bright } else { LedValue::Dim };
        let channel_led = if mode == PageMode::Channel { LedValue::Bright } else { LedValue::Dim };

        self.encdr.set_led_in_group(self.device_id, "button_leds", "plugin", plugin_led);
        self.encdr.set_led_in_group(self.device_id, "button_leds", "mixer", mixer_led);
        self.encdr.set_led_in_group(self.device_id, "button_leds", "channel", channel_led);

        if mode == PageMode::Channel {
            let mute_val = if self.state.channel_muted { LedValue::Bright } else { LedValue::Dim };
            let solo_val = if self.state.channel_solo { LedValue::Bright } else { LedValue::Dim };
            self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", mute_val);
            self.encdr.set_led_in_group(self.device_id, "button_leds", "solo", solo_val);
        } else {
            let mute_val = if self.state.track_muted { LedValue::Bright } else { LedValue::Dim };
            self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", mute_val);
            self.encdr.set_led_in_group(self.device_id, "button_leds", "solo", LedValue::Dim);
        }

        self.update_top_button_leds();
        self.update_compass_leds();
    }

    /// The strip index of the selected track, falling back to the last known one
    /// (e.g. while the master channel is focused and no strip track is selected).
    fn selected_idx(&self) -> Option<usize> {
        self.state
            .strip
            .iter()
            .position(|t| t.exists && t.selected)
            .or_else(|| self.state.strip.get(self.state.strip_selected_idx).filter(|t| t.exists).map(|t| t.index))
    }

    /// Each compass LED around the 4D encoder shows the colour of the track that
    /// pressing in that direction moves to (off when there is nowhere to go).
    pub fn update_compass_leds(&mut self) {
        let vals = if self.state.page_mode != PageMode::Channel {
            [LedValue::Off; 4]
        } else if self.state.channel_is_master {
            // Any direction leaves the master channel and returns to the previous track.
            let back = self
                .selected_idx()
                .map(|i| parse_hex_to_led(&self.state.strip[i].color))
                .unwrap_or(LedValue::Dim);
            [back; 4]
        } else if let Some(sel) = self.selected_idx() {
            let t = nav::targets(&self.state.strip, sel);
            let led = |i: Option<usize>| i.map(|i| parse_hex_to_led(&self.state.strip[i].color)).unwrap_or(LedValue::Off);
            // Rotation/tilt can still leave the strip bank via Bitwig's own cursor (huge projects only).
            let left = if t.left.is_none() && self.state.strip_can_scroll_back { LedValue::Dim } else { led(t.left) };
            let right = if t.right.is_none() && self.state.strip_can_scroll_forw { LedValue::Dim } else { led(t.right) };
            let up = if t.up.is_none() && self.state.strip[sel].depth > 0 { LedValue::Dim } else { led(t.up) };
            [up, left, right, led(t.down)]
        } else {
            [LedValue::Off; 4]
        };

        if self.state.compass_leds == Some(vals) {
            return;
        }
        self.state.compass_leds = Some(vals);
        for (name, val) in ["encoder_up", "encoder_left", "encoder_right", "encoder_down"].iter().zip(vals) {
            self.encdr.set_led_in_group(self.device_id, "button_leds", name, val);
        }
    }

    fn select_track(&mut self, idx: usize) {
        self.send_osc(format!("/strip/{}/select", idx), vec![]);
    }

    fn set_group_expanded(&self, idx: usize, expanded: bool) {
        self.send_osc(format!("/strip/{}/expand", idx), vec![OscType::Int(expanded as i32)]);
    }

    /// Handle a 4D encoder direction (tilt or rotation) on the channel page.
    pub fn navigate(&mut self, dir: NavDir) {
        self.state.nav_seq += 1;
        self.state.dirty = true;
        if self.state.channel_is_master {
            self.send_osc("/channel/master/toggle".to_string(), vec![]);
            return;
        }
        self.state.pending_enter = None;
        let Some(sel) = self.selected_idx() else {
            // Nothing in the strip is selected; let Bitwig move its cursor.
            let addr = if matches!(dir, NavDir::Left | NavDir::Up) { "/track/nav/prev" } else { "/track/nav/next" };
            self.send_osc(addr.to_string(), vec![]);
            return;
        };
        let tracks = &self.state.strip;
        match dir {
            NavDir::Left | NavDir::Right => {
                let d = if dir == NavDir::Left { -1 } else { 1 };
                match nav::step(tracks, sel, d) {
                    Some(i) => self.select_track(i),
                    // At the edge of the strip bank: Bitwig's cursor scrolls it for us.
                    None => self.send_osc(if d < 0 { "/track/nav/prev" } else { "/track/nav/next" }.to_string(), vec![]),
                }
            }
            NavDir::Up => {
                let t = &tracks[sel];
                if let Some(p) = nav::parent_of(tracks, sel) {
                    self.select_track(p);
                    self.set_group_expanded(p, false);
                } else if t.depth > 0 {
                    // Parent is outside the strip bank.
                    self.send_osc("/channel/nav/parent".to_string(), vec![]);
                } else if t.is_group && t.is_group_expanded {
                    self.set_group_expanded(sel, false);
                }
            }
            NavDir::Down => {
                let t = &tracks[sel];
                if !t.is_group {
                    return;
                }
                if t.is_group_expanded {
                    if let Some(c) = nav::first_child(tracks, sel) {
                        self.select_track(c);
                        return;
                    }
                } else {
                    self.set_group_expanded(sel, true);
                }
                // Children may only appear in the bank once Bitwig has expanded the group.
                self.state.pending_enter = Some(PendingEnter {
                    group_idx: sel,
                    group_name: t.name.clone(),
                    since: Instant::now(),
                });
                self.resolve_pending_enter();
            }
        }
    }

    /// Enter the first child of a group we just expanded, as soon as it is loaded.
    fn resolve_pending_enter(&mut self) {
        let Some(p) = &self.state.pending_enter else { return };
        let still_valid = p.since.elapsed() < Duration::from_millis(1500)
            && self.state.strip.get(p.group_idx).is_some_and(|t| t.exists && t.name == p.group_name);
        if !still_valid {
            self.state.pending_enter = None;
            return;
        }
        let g = p.group_idx;
        if self.state.strip[g].is_group_expanded {
            if let Some(c) = nav::first_child(&self.state.strip, g) {
                self.state.pending_enter = None;
                self.select_track(c);
            }
        }
    }

    /// `/strip/...` messages from the script's strip bank.
    fn handle_strip_osc(&mut self, path: &str, args: &[OscType]) {
        let flag = args.first() == Some(&OscType::Int(1));
        let mut parts = path.split('/');
        let (Some(head), field) = (parts.next(), parts.next()) else { return };
        match (head, field) {
            ("offset", None) => {
                if let Some(OscType::Int(off)) = args.first() {
                    self.state.strip_offset = *off;
                }
            }
            ("can_scroll_back", None) => self.state.strip_can_scroll_back = flag,
            ("can_scroll_forw", None) => self.state.strip_can_scroll_forw = flag,
            (idx, Some(field)) => {
                let Ok(idx) = idx.parse::<usize>() else { return };
                if idx >= 1024 {
                    return;
                }
                let strip = &mut self.state.strip;
                while strip.len() <= idx {
                    strip.push(TrackData { index: strip.len(), ..Default::default() });
                }
                let t = &mut strip[idx];
                match (field, args.first()) {
                    ("name", Some(OscType::String(v))) => t.name = v.clone(),
                    ("color", Some(OscType::String(v))) => t.color = v.clone(),
                    ("exists", _) => {
                        t.exists = flag;
                        if !flag {
                            t.selected = false;
                        }
                    }
                    ("activated", _) => t.activated = flag,
                    ("is_group", _) => t.is_group = flag,
                    ("is_group_expanded", _) => t.is_group_expanded = flag,
                    ("depth", Some(OscType::Int(d))) => t.depth = (*d).max(0) as u32,
                    ("selected", _) => {
                        t.selected = flag;
                        if flag {
                            self.state.strip_selected_idx = idx;
                        }
                    }
                    _ => return,
                }
            }
            _ => return,
        }
        self.state.dirty = true;
    }

    pub fn update_top_button_leds(&self) {
        match self.state.page_mode {
            PageMode::Mixer => {
                for (i, &name) in self.top_button_names.iter().enumerate() {
                    let val = if i < self.state.tracks.len() && self.state.tracks[i].selected {
                        LedValue::Bright
                    } else if i < self.state.tracks.len() && (self.state.tracks[i].solo || self.state.tracks[i].muted) {
                        LedValue::Bright
                    } else if i < self.state.tracks.len() && self.state.tracks[i].exists {
                        LedValue::Dim
                    } else {
                        LedValue::Off
                    };
                    self.encdr.set_led_in_group(self.device_id, "button_leds", name, val);
                }
            }
            PageMode::Device => {
                for &name in &self.top_button_names {
                    self.encdr.set_led_in_group(self.device_id, "button_leds", name, LedValue::Dim);
                }
            }
            PageMode::Channel => {
                let max_page = (self.state.sends_total.saturating_sub(1)) / 6;
                for (i, &name) in self.top_button_names.iter().enumerate() {
                    let val = if i == 0 {
                        // Button 1: REC ARM
                        if self.state.channel_arm { LedValue::Bright } else { LedValue::Dim }
                    } else if i == 3 {
                        // Button 4: MONITOR
                        LedValue::Dim
                    } else if i == 4 {
                        // Button 5: SEND PAGE PREV (< PAGE)
                        if self.state.sends_page > 0 { LedValue::Bright } else { LedValue::Dim }
                    } else if i == 5 {
                        // Button 6: SEND PAGE NEXT (PAGE >)
                        if self.state.sends_page < max_page { LedValue::Bright } else { LedValue::Dim }
                    } else if i == 7 {
                        // Button 8: MASTER
                        if self.state.channel_is_master { LedValue::Bright } else { LedValue::Dim }
                    } else {
                        LedValue::Off
                    };
                    self.encdr.set_led_in_group(self.device_id, "button_leds", name, val);
                }
            }
        }
    }

    pub fn send_osc(&self, addr: String, args: Vec<OscType>) {
        let msg = encoder::encode(&OscPacket::Message(OscMessage { addr, args })).unwrap();
        let _ = self.osc_sender.send_to(&msg, "127.0.0.1:8082");
    }

    pub fn handle_osc_message(&mut self, msg: OscMessage) {
        if msg.addr == "/transport/play" {
            let active = msg.args.first() == Some(&OscType::Int(1));
            let play_val = if active { LedValue::Bright } else { LedValue::Dim };
            let stop_val = if active { LedValue::Dim } else { LedValue::Bright };
            self.encdr.set_led_in_group(self.device_id, "button_leds", "play", play_val);
            self.encdr.set_led_in_group(self.device_id, "button_leds", "stop", stop_val);
        } else if msg.addr == "/transport/rec" {
            let active = msg.args.first() == Some(&OscType::Int(1));
            let rec_val = if active { LedValue::Bright } else { LedValue::Dim };
            self.encdr.set_led_in_group(self.device_id, "button_leds", "rec", rec_val);
        } else if msg.addr == "/track/mute" {
            let muted = msg.args.first() == Some(&OscType::Int(1));
            if self.state.track_muted != muted {
                self.state.track_muted = muted;
                self.state.dirty = true;
                let mute_val = if muted { LedValue::Bright } else { LedValue::Dim };
                self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", mute_val);
            }
        } else if msg.addr == "/device/name" {
            if let Some(OscType::String(name)) = msg.args.first() {
                self.state.device_name = name.clone();
                self.state.dirty = true;
            }
        } else if msg.addr.starts_with("/device/param/") {
            let parts: Vec<&str> = msg.addr.split('/').collect();
            if parts.len() == 5 {
                if let Ok(idx) = parts[3].parse::<usize>() {
                    if idx < 8 {
                        match parts[4] {
                            "name" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.param_names[idx] = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "display" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.param_displays[idx] = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "value" => {
                                let val = match msg.args.first() {
                                    Some(OscType::Float(v)) => *v,
                                    Some(OscType::Double(v)) => *v as f32,
                                    Some(OscType::Int(v)) => *v as f32,
                                    _ => 0.0,
                                };
                                self.state.param_values[idx] = val;
                                self.state.dirty = true;
                            }
                            "modulated" => {
                                let val = match msg.args.first() {
                                    Some(OscType::Float(v)) => *v,
                                    Some(OscType::Double(v)) => *v as f32,
                                    Some(OscType::Int(v)) => *v as f32,
                                    _ => 0.0,
                                };
                                self.state.param_modulated[idx] = val;
                                self.state.dirty = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
        } else if let Some(rest) = msg.addr.strip_prefix("/strip/") {
            self.handle_strip_osc(rest, &msg.args);
        } else if msg.addr.starts_with("/track/bank/") {
            let parts: Vec<&str> = msg.addr.split('/').collect();
            if parts.len() >= 4 {
                if parts[3] == "offset" {
                    if let Some(OscType::Int(off)) = msg.args.first() {
                        self.state.bank_offset = *off;
                        self.state.dirty = true;
                    }
                } else if parts[3] == "can_scroll_back" {
                    let can = msg.args.first() == Some(&OscType::Int(1));
                    self.state.bank_can_scroll_back = can;
                    self.state.dirty = true;
                } else if parts[3] == "can_scroll_forw" {
                    let can = msg.args.first() == Some(&OscType::Int(1));
                    self.state.bank_can_scroll_forw = can;
                    self.state.dirty = true;
                } else if let Ok(idx) = parts[3].parse::<usize>() {
                    if parts.len() == 5 {
                        if idx >= self.state.tracks.len() {
                            for new_i in self.state.tracks.len()..=idx {
                                self.state.tracks.push(TrackData {
                                    index: new_i,
                                    name: format!("Track {}", new_i + 1),
                                    ..Default::default()
                                });
                            }
                        }
                        let field = parts[4];
                        match field {
                            "name" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.tracks[idx].name = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "color" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.tracks[idx].color = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "volume" => {
                                if let Some(v) = extract_float(msg.args.first()) {
                                    self.state.tracks[idx].volume = v;
                                    self.state.dirty = true;
                                }
                            }
                            "volume_display" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.tracks[idx].volume_display = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "pan" => {
                                if let Some(v) = extract_float(msg.args.first()) {
                                    self.state.tracks[idx].pan = v;
                                    self.state.dirty = true;
                                }
                            }
                            "pan_display" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.tracks[idx].pan_display = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "mute" => {
                                let m = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].muted = m;
                                self.state.dirty = true;
                                self.update_top_button_leds();
                            }
                            "solo" => {
                                let s = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].solo = s;
                                self.state.dirty = true;
                                self.update_top_button_leds();
                            }
                            "arm" => {
                                let a = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].arm = a;
                                self.state.dirty = true;
                            }
                            "exists" => {
                                let e = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].exists = e;
                                self.state.dirty = true;
                                self.update_top_button_leds();
                            }
                            "activated" => {
                                self.state.tracks[idx].activated = msg.args.first() == Some(&OscType::Int(1));
                                self.state.dirty = true;
                            }
                            "is_group" => {
                                let g = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].is_group = g;
                                self.state.dirty = true;
                            }
                            "is_group_expanded" => {
                                let ge = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].is_group_expanded = ge;
                                self.state.dirty = true;
                            }
                            "selected" => {
                                let s = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].selected = s;
                                if s {
                                    self.state.selected_track_idx = idx;
                                }
                                self.state.dirty = true;
                            }
                            "depth" => {
                                if let Some(OscType::Int(d)) = msg.args.first() {
                                    self.state.tracks[idx].depth = (*d).max(0) as u32;
                                    self.state.dirty = true;
                                }
                            }
                            "has_parent" => {
                                let hp = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].has_parent = hp;
                                self.state.dirty = true;
                            }
                            "parent_color" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.tracks[idx].parent_color = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "parent_name" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.tracks[idx].parent_name = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "has_grandparent" => {
                                let hg = msg.args.first() == Some(&OscType::Int(1));
                                self.state.tracks[idx].has_grandparent = hg;
                                self.state.dirty = true;
                            }
                            "grandparent_color" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.tracks[idx].grandparent_color = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "grandparent_name" => {
                                if let Some(OscType::String(val)) = msg.args.first() {
                                    self.state.tracks[idx].grandparent_name = val.clone();
                                    self.state.dirty = true;
                                }
                            }
                            "meter" => {
                                if let Some(v) = extract_float(msg.args.first()) {
                                    self.state.tracks[idx].meter = v;
                                    self.state.dirty = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        } else if msg.addr.starts_with("/channel/") {
            let parts: Vec<&str> = msg.addr.split('/').collect();
            if parts.len() >= 3 {
                match parts[2] {
                    "is_master" => {
                        let m = msg.args.first() == Some(&OscType::Int(1));
                        self.state.channel_is_master = m;
                        self.state.dirty = true;
                        self.update_top_button_leds();
                    }
                    "is_group" => {
                        let g = msg.args.first() == Some(&OscType::Int(1));
                        self.state.channel_is_group = g;
                        self.state.dirty = true;
                    }
                    "is_group_expanded" => {
                        let ge = msg.args.first() == Some(&OscType::Int(1));
                        self.state.channel_is_group_expanded = ge;
                        self.state.dirty = true;
                    }
                    "master" => {
                        if parts.len() >= 4 {
                            match parts[3] {
                                "name" => {
                                    if let Some(OscType::String(val)) = msg.args.first() {
                                        self.state.master_name = val.clone();
                                        self.state.dirty = true;
                                    }
                                }
                                "color" => {
                                    if let Some(OscType::String(val)) = msg.args.first() {
                                        self.state.master_color = val.clone();
                                        self.state.dirty = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "monitor" => {
                        if parts.len() >= 4 && parts[3] == "mode" {
                            if let Some(OscType::String(val)) = msg.args.first() {
                                self.state.channel_monitor_mode = val.clone();
                                self.state.dirty = true;
                            }
                        }
                    }
                    "name" => {
                        if let Some(OscType::String(val)) = msg.args.first() {
                            self.state.channel_name = val.clone();
                            self.state.channel_max_peak = 0.0;
                            self.state.dirty = true;
                            self.update_top_button_leds();
                        }
                    }
                    "color" => {
                        if let Some(OscType::String(val)) = msg.args.first() {
                            self.state.channel_color = val.clone();
                            self.state.dirty = true;
                        }
                    }
                    "volume" => {
                        if let Some(v) = extract_float(msg.args.first()) {
                            self.state.channel_volume = v;
                            self.state.dirty = true;
                        }
                    }
                    "volume_display" => {
                        if let Some(OscType::String(val)) = msg.args.first() {
                            self.state.channel_volume_display = val.clone();
                            self.state.dirty = true;
                        }
                    }
                    "pan" => {
                        if let Some(v) = extract_float(msg.args.first()) {
                            self.state.channel_pan = v;
                            self.state.dirty = true;
                        }
                    }
                    "pan_display" => {
                        if let Some(OscType::String(val)) = msg.args.first() {
                            self.state.channel_pan_display = val.clone();
                            self.state.dirty = true;
                        }
                    }
                    "mute" => {
                        let is_muted = msg.args.first() == Some(&OscType::Int(1));
                        self.state.channel_muted = is_muted;
                        self.state.dirty = true;
                        if self.state.page_mode == PageMode::Channel {
                            let val = if is_muted { LedValue::Bright } else { LedValue::Dim };
                            self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", val);
                        }
                        self.update_top_button_leds();
                    }
                    "solo" => {
                        let is_solo = msg.args.first() == Some(&OscType::Int(1));
                        self.state.channel_solo = is_solo;
                        self.state.dirty = true;
                        if self.state.page_mode == PageMode::Channel {
                            let val = if is_solo { LedValue::Bright } else { LedValue::Dim };
                            self.encdr.set_led_in_group(self.device_id, "button_leds", "solo", val);
                        }
                        self.update_top_button_leds();
                    }
                    "arm" => {
                        self.state.channel_arm = msg.args.first() == Some(&OscType::Int(1));
                        self.state.dirty = true;
                        self.update_top_button_leds();
                    }
                    "meter" => {
                        let (l_val, r_val) = if msg.args.len() >= 2 {
                            (extract_float(msg.args.get(0)).unwrap_or(0.0), extract_float(msg.args.get(1)).unwrap_or(0.0))
                        } else if let Some(v) = extract_float(msg.args.first()) {
                            (v, v)
                        } else {
                            (0.0, 0.0)
                        };
                        self.state.channel_meter = l_val.max(r_val);
                        self.state.channel_meter_l = l_val;
                        self.state.channel_meter_r = r_val;
                        if l_val > self.state.channel_max_peak {
                            self.state.channel_max_peak = l_val;
                        }
                        if r_val > self.state.channel_max_peak {
                            self.state.channel_max_peak = r_val;
                        }
                        self.state.dirty = true;
                    }
                    "sends" => {
                        if parts.len() == 4 && parts[3] == "total" {
                            if let Some(OscType::Int(val)) = msg.args.first() {
                                self.state.sends_total = (*val).max(0) as usize;
                                self.state.dirty = true;
                                self.update_top_button_leds();
                            }
                        }
                    }
                    "send" => {
                        if parts.len() == 5 {
                            if let Ok(idx) = parts[3].parse::<usize>() {
                                if idx < 12 {
                                    match parts[4] {
                                        "name" => {
                                            if let Some(OscType::String(val)) = msg.args.first() {
                                                self.state.sends[idx].name = val.clone();
                                                self.state.dirty = true;
                                            }
                                        }
                                        "value" => {
                                            if let Some(v) = extract_float(msg.args.first()) {
                                                self.state.sends[idx].value = v;
                                                self.state.dirty = true;
                                            }
                                        }
                                        "display" => {
                                            if let Some(OscType::String(val)) = msg.args.first() {
                                                self.state.sends[idx].display = val.clone();
                                                self.state.dirty = true;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn apply_screen_encoder(&self, idx: usize, delta: f32, fine: bool) {
        let base_mult = if fine { 100.0 } else { 10.0 };
        let multiplier = if self.state.shift_pressed { base_mult * 0.1 } else { base_mult };
        let val = delta * multiplier;

        match self.state.page_mode {
            PageMode::Device => {
                let addr = format!("/device/param/{}/inc", idx);
                self.send_osc(addr, vec![OscType::Float(val)]);
            }
            PageMode::Mixer => {
                if self.state.shift_pressed {
                    let addr = format!("/track/bank/{}/pan/inc", idx);
                    self.send_osc(addr, vec![OscType::Float(val)]);
                } else {
                    let addr = format!("/track/bank/{}/volume/inc", idx);
                    self.send_osc(addr, vec![OscType::Float(val)]);
                }
            }
            PageMode::Channel => {
                if idx == 0 {
                    self.send_osc("/channel/volume/inc".to_string(), vec![OscType::Float(val)]);
                } else if idx == 1 {
                    self.send_osc("/channel/pan/inc".to_string(), vec![OscType::Float(val)]);
                } else if idx >= 2 && idx < 8 {
                    let send_idx = self.state.sends_page * 6 + (idx - 2);
                    let addr = format!("/channel/send/{}/inc", send_idx);
                    self.send_osc(addr, vec![OscType::Float(val)]);
                }
            }
        }
    }

    pub fn handle_encdr_event(&mut self, event: Event) {
        match event {
            Event::EncoderFine { name, delta, .. } => {
                if let Some(idx) = self.encoder_names.iter().position(|&n| n == name) {
                    if delta.abs() <= 0.7 {
                        self.apply_screen_encoder(idx, delta as f32, true);
                        self.modifier_used_as_gesture = true;
                    }
                }
            }
            Event::Encoder { name, delta, .. } => {
                if let Some(idx) = self.encoder_names.iter().position(|&n| n == name) {
                    self.apply_screen_encoder(idx, delta as f32, false);
                    self.modifier_used_as_gesture = true;
                } else if name == "main_encoder" {
                    // 4-D Encoder rotation: previous / next track
                    if self.state.page_mode == PageMode::Channel {
                        if delta != 0 {
                            self.navigate(if delta > 0 { NavDir::Right } else { NavDir::Left });
                        }
                    } else if delta > 0 {
                        self.send_osc("/track/nav/next".to_string(), vec![]);
                    } else if delta < 0 {
                        self.send_osc("/track/nav/prev".to_string(), vec![]);
                    }
                }
            }
            Event::Touch { name: "encoder_touch", touched, .. } => {
                self.state.nav_touched = touched;
                self.state.dirty = true;
            }
            Event::Button { name, pressed, .. } => {
                if name == "shift" {
                    self.state.shift_pressed = pressed;
                    let shift_val = if pressed { LedValue::Bright } else { LedValue::Dim };
                    self.encdr.set_led_in_group(self.device_id, "button_leds", "shift", shift_val);
                } else if name == "plugin" {
                    if pressed {
                        self.set_page_mode(PageMode::Device);
                    }
                } else if name == "mixer" {
                    if pressed {
                        self.set_page_mode(PageMode::Mixer);
                    }
                } else if name == "channel" {
                    if pressed {
                        self.set_page_mode(PageMode::Channel);
                    }
                } else if name == "mute" {
                    self.state.mute_held = pressed;
                    if self.state.page_mode == PageMode::Channel {
                        if pressed {
                            self.send_osc("/channel/mute/toggle".to_string(), vec![]);
                        }
                    } else {
                        let led_val = if pressed || self.state.track_muted { LedValue::Bright } else { LedValue::Dim };
                        self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", led_val);
                    }
                } else if name == "solo" {
                    self.state.solo_held = pressed;
                    if self.state.page_mode == PageMode::Channel {
                        if pressed {
                            self.send_osc("/channel/solo/toggle".to_string(), vec![]);
                        }
                    } else {
                        let led_val = if pressed { LedValue::Bright } else { LedValue::Dim };
                        self.encdr.set_led_in_group(self.device_id, "button_leds", "solo", led_val);
                    }
                } else if let Some(idx) = self.top_button_names.iter().position(|&n| n == name) {
                    // Top 1 - 8 Screen Buttons
                    if pressed {
                        match self.state.page_mode {
                            PageMode::Mixer => {
                                if self.state.mute_held {
                                    let addr = format!("/track/bank/{}/mute/toggle", idx);
                                    self.send_osc(addr, vec![]);
                                } else if self.state.solo_held {
                                    let addr = format!("/track/bank/{}/solo/toggle", idx);
                                    self.send_osc(addr, vec![]);
                                } else if self.state.shift_pressed {
                                    let addr = format!("/track/bank/{}/arm/toggle", idx);
                                    self.send_osc(addr, vec![]);
                                } else {
                                    let addr = format!("/track/bank/{}/select", idx);
                                    self.send_osc(addr, vec![]);
                                }
                            }
                            PageMode::Channel => {
                                if idx == 0 {
                                    // Button 1: REC ARM
                                    self.send_osc("/channel/arm/toggle".to_string(), vec![]);
                                } else if idx == 3 {
                                    // Button 4: MONITOR
                                    self.send_osc("/channel/monitor/cycle".to_string(), vec![]);
                                } else if idx == 4 {
                                    // Button 5: SEND PAGE PREV (< PAGE)
                                    if self.state.sends_page > 0 {
                                        self.state.sends_page -= 1;
                                        self.state.dirty = true;
                                        self.update_top_button_leds();
                                    }
                                } else if idx == 5 {
                                    // Button 6: SEND PAGE NEXT (PAGE >)
                                    let max_page = (self.state.sends_total.saturating_sub(1)) / 6;
                                    if self.state.sends_page < max_page {
                                        self.state.sends_page += 1;
                                        self.state.dirty = true;
                                        self.update_top_button_leds();
                                    }
                                } else if idx == 7 {
                                    // Button 8: MASTER TOGGLE
                                    self.send_osc("/channel/master/toggle".to_string(), vec![]);
                                }
                            }
                            PageMode::Device => {}
                        }
                    }
                } else if pressed {
                    self.modifier_used_as_gesture = true;
                    if self.state.shift_pressed && (name == "volume" || name == "fixed_vel") {
                        self.state.channel_max_peak = 0.0;
                        self.state.dirty = true;
                    } else if name == "play" {
                        self.send_osc("/transport/play".to_string(), vec![]);
                    } else if name == "stop" {
                        self.send_osc("/transport/stop".to_string(), vec![]);
                    } else if name == "rec" {
                        let addr = if self.state.shift_pressed {
                            "/transport/rec_count_in".to_string()
                        } else {
                            "/transport/rec".to_string()
                        };
                        self.send_osc(addr, vec![]);
                    } else if name == "restart" {
                        let addr = if self.state.shift_pressed {
                            "/transport/loop_toggle".to_string()
                        } else {
                            "/transport/restart".to_string()
                        };
                        self.send_osc(addr, vec![]);
                        self.encdr.set_led_in_group(self.device_id, "button_leds", "restart", LedValue::Bright);
                    } else if name == "arrow_left" {
                        match self.state.page_mode {
                            PageMode::Device => {
                                self.send_osc("/device/nav/prev".to_string(), vec![]);
                            }
                            PageMode::Mixer | PageMode::Channel => {
                                if self.state.shift_pressed {
                                    self.send_osc("/track/nav/prev".to_string(), vec![]);
                                } else {
                                    self.send_osc("/track/bank/scroll_back".to_string(), vec![]);
                                }
                            }
                        }
                    } else if name == "arrow_right" {
                        match self.state.page_mode {
                            PageMode::Device => {
                                self.send_osc("/device/nav/next".to_string(), vec![]);
                            }
                            PageMode::Mixer | PageMode::Channel => {
                                if self.state.shift_pressed {
                                    self.send_osc("/track/nav/next".to_string(), vec![]);
                                } else {
                                    self.send_osc("/track/bank/scroll_forw".to_string(), vec![]);
                                }
                            }
                        }
                    } else if name == "encoder_press" {
                        if self.state.page_mode == PageMode::Channel {
                            // In Channel mode, 4-D encoder press toggles group open/close
                            self.send_osc("/channel/group/toggle".to_string(), vec![]);
                        } else if self.state.page_mode == PageMode::Mixer {
                            self.set_page_mode(PageMode::Device);
                        } else {
                            self.set_page_mode(PageMode::Mixer);
                        }
                    } else if self.state.page_mode == PageMode::Channel
                        && matches!(name, "encoder_left" | "encoder_right" | "encoder_up" | "encoder_down")
                    {
                        self.navigate(match name {
                            "encoder_left" => NavDir::Left,
                            "encoder_right" => NavDir::Right,
                            "encoder_up" => NavDir::Up,
                            _ => NavDir::Down,
                        });
                    } else if name == "encoder_left" {
                        self.send_osc("/track/bank/scroll_back".to_string(), vec![]);
                    } else if name == "encoder_right" {
                        self.send_osc("/track/bank/scroll_forw".to_string(), vec![]);
                    } else if name == "encoder_up" {
                        self.send_osc("/track/nav/prev".to_string(), vec![]);
                    } else if name == "encoder_down" {
                        self.send_osc("/track/nav/next".to_string(), vec![]);
                    }
                } else if !pressed {
                    if name == "restart" {
                        self.encdr.set_led_in_group(self.device_id, "button_leds", "restart", LedValue::Dim);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn check_and_send_state(&mut self) {
        if self.state.dirty {
            self.resolve_pending_enter();
            self.update_compass_leds();

            let page_str = match self.state.page_mode {
                PageMode::Device => "device",
                PageMode::Mixer => "mixer",
                PageMode::Channel => "channel",
            };

            let state_json = serde_json::json!({
                "page": page_str,
                "device": self.state.device_name,
                "names": self.state.param_names,
                "displays": self.state.param_displays,
                "values": self.state.param_values,
                "modulated": self.state.param_modulated,
                "track_muted": self.state.track_muted,

                "tracks": self.state.tracks,
                "bankOffset": self.state.bank_offset,
                "bankCanScrollBack": self.state.bank_can_scroll_back,
                "bankCanScrollForw": self.state.bank_can_scroll_forw,
                "strip": self.state.strip.iter().filter(|t| t.exists).collect::<Vec<_>>(),
                "stripSelectedIndex": self.state.strip_selected_idx,

                "selectedTrackIndex": self.state.selected_track_idx,
                "selectedTrackName": self.state.channel_name,
                "selectedTrackColor": self.state.channel_color,
                "selectedTrackVolume": self.state.channel_volume,
                "selectedTrackVolumeDisplay": self.state.channel_volume_display,
                "selectedTrackPan": self.state.channel_pan,
                "selectedTrackPanDisplay": self.state.channel_pan_display,
                "selectedTrackMeter": self.state.channel_meter,
                "selectedTrackMeterL": self.state.channel_meter_l,
                "selectedTrackMeterR": self.state.channel_meter_r,
                "selectedTrackMaxPeak": self.state.channel_max_peak,
                "selectedTrackMuted": self.state.channel_muted,
                "selectedTrackSolo": self.state.channel_solo,
                "selectedTrackArm": self.state.channel_arm,
                "channelIsGroup": self.state.channel_is_group,
                "channelIsGroupExpanded": self.state.channel_is_group_expanded,
                "channelIsMaster": self.state.channel_is_master,
                "channelMonitorMode": self.state.channel_monitor_mode,
                "masterName": self.state.master_name,
                "masterColor": self.state.master_color,
                "sends": self.state.sends,
                "sendsPage": self.state.sends_page,
                "sendsTotal": self.state.sends_total,
                "navTouched": self.state.nav_touched,
                "navSeq": self.state.nav_seq,
            });

            let _ = self.state_tx.send(state_json);
            self.state.dirty = false;
        }
    }
}

fn parse_hex_to_led(hex: &str) -> LedValue {
    let clean = hex.trim_start_matches('#');
    if clean.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&clean[0..2], 16),
            u8::from_str_radix(&clean[2..4], 16),
            u8::from_str_radix(&clean[4..6], 16),
        ) {
            return LedValue::Rgb { r, g, b };
        }
    }
    LedValue::Dim
}

fn extract_float(arg: Option<&OscType>) -> Option<f32> {
    match arg {
        Some(OscType::Float(v)) => Some(*v),
        Some(OscType::Double(v)) => Some(*v as f32),
        Some(OscType::Int(v)) => Some(*v as f32),
        _ => None,
    }
}
