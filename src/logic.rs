use std::net::UdpSocket;
use crossbeam_channel::Sender;
use rosc::{OscPacket, OscType, OscMessage, encoder};
use serde_json::Value;
use std::sync::Arc;
use encdr::{Encdr, DeviceId, LedValue, Event};

#[derive(Clone, PartialEq, Eq)]
pub enum PageShape {
    Channel, // e.g. device params
    Spread,  // e.g. track volumes
    Global,  // e.g. master tempo
}

#[derive(Clone)]
pub struct PageDescriptor {
    pub name: String,
    pub shape: PageShape,
    pub osc_path: String,
}

#[derive(Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Mute,
    Solo,
}

pub struct LogicState {
    pub device_name: String,
    pub param_names: [String; 8],
    pub param_displays: [String; 8],
    pub param_values: [f32; 8],
    pub param_modulated: [f32; 8],
    pub shift_pressed: bool,
    pub active_mode: AppMode,
    pub latched_mode: AppMode,
    pub active_page: PageDescriptor,
    pub track_muted: bool,
    pub dirty: bool,
}

impl Default for LogicState {
    fn default() -> Self {
        Self {
            device_name: String::new(),
            param_names: Default::default(),
            param_displays: Default::default(),
            param_values: [0.0; 8],
            param_modulated: [0.0; 8],
            shift_pressed: false,
            active_mode: AppMode::Normal,
            latched_mode: AppMode::Normal,
            active_page: PageDescriptor {
                name: "Device".to_string(),
                shape: PageShape::Channel,
                osc_path: "/device/param".to_string(),
            },
            track_muted: false,
            dirty: true,
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
    modifier_used_as_gesture: bool,
}

impl AppLogic {
    pub fn new(state_tx: Sender<Value>, encdr: Arc<Encdr>, device_id: DeviceId) -> Self {
        let osc_sender = UdpSocket::bind("0.0.0.0:0").unwrap();
        let app = Self {
            state: LogicState::default(),
            osc_sender,
            state_tx,
            encdr,
            device_id,
            encoder_names: [
                "screen_encoder_1", "screen_encoder_2", "screen_encoder_3", "screen_encoder_4",
                "screen_encoder_5", "screen_encoder_6", "screen_encoder_7", "screen_encoder_8",
            ],
            modifier_used_as_gesture: false,
        };

        // Broadcast refresh on initialization so host pushes full state
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.send_osc("/refresh".to_string(), vec![]);
        app
    }

    pub fn send_osc(&self, addr: String, args: Vec<OscType>) {
        let msg = encoder::encode(&OscPacket::Message(OscMessage { addr, args })).unwrap();
        let _ = self.osc_sender.send_to(&msg, "127.0.0.1:8082");
    }

    pub fn handle_osc_message(&mut self, msg: OscMessage) {
        if msg.addr == "/transport/play" {
            let active = msg.args.get(0) == Some(&OscType::Int(1));
            let play_val = if active { LedValue::Single(255) } else { LedValue::Single(0) };
            self.encdr.set_led_in_group(self.device_id, "button_leds", "play", play_val);
        } else if msg.addr == "/transport/rec" {
            let active = msg.args.get(0) == Some(&OscType::Int(1));
            let rec_val = if active { LedValue::Single(255) } else { LedValue::Single(0) };
            self.encdr.set_led_in_group(self.device_id, "button_leds", "rec", rec_val);
        } else if msg.addr == "/track/mute" {
            let muted = msg.args.get(0) == Some(&OscType::Int(1));
            if self.state.track_muted != muted {
                self.state.track_muted = muted;
                self.state.dirty = true;
            }
        } else if msg.addr == "/device/name" {
            if let Some(OscType::String(name)) = msg.args.get(0) {
                self.state.device_name = name.clone();
                self.state.dirty = true;
            }
        } else if msg.addr.starts_with("/device/param/") {
            let parts: Vec<&str> = msg.addr.split('/').collect();
            if parts.len() == 5 {
                if let Ok(idx) = parts[3].parse::<usize>() {
                    if idx < 8 {
                        if parts[4] == "name" {
                            if let Some(OscType::String(val)) = msg.args.get(0) {
                                self.state.param_names[idx] = val.clone();
                                self.state.dirty = true;
                            }
                        } else if parts[4] == "display" {
                            if let Some(OscType::String(val)) = msg.args.get(0) {
                                self.state.param_displays[idx] = val.clone();
                                self.state.dirty = true;
                            }
                        } else if parts[4] == "value" {
                            let val = match msg.args.get(0) {
                                Some(OscType::Float(v)) => *v,
                                Some(OscType::Double(v)) => *v as f32,
                                Some(OscType::Int(v)) => *v as f32,
                                _ => 0.0,
                            };
                            self.state.param_values[idx] = val;
                            self.state.dirty = true;
                        } else if parts[4] == "modulated" {
                            let val = match msg.args.get(0) {
                                Some(OscType::Float(v)) => *v,
                                Some(OscType::Double(v)) => *v as f32,
                                Some(OscType::Int(v)) => *v as f32,
                                _ => 0.0,
                            };
                            self.state.param_modulated[idx] = val;
                            self.state.dirty = true;
                        }
                    }
                }
            }
        }
    }

    pub fn apply_encoder_action(&self, idx: usize, delta: f32, fine: bool) {
        let base_mult = if fine { 100.0 } else { 10.0 };
        let multiplier = if self.state.shift_pressed { base_mult * 0.1 } else { base_mult };
        match self.state.active_page.shape {
            PageShape::Channel => {
                let addr = format!("{}/{}/inc", self.state.active_page.osc_path, idx);
                self.send_osc(addr, vec![OscType::Float(delta * multiplier)]);
            }
            PageShape::Spread => {
                let addr = format!("{}/{}/inc", self.state.active_page.osc_path, idx);
                self.send_osc(addr, vec![OscType::Float(delta * multiplier)]);
            }
            PageShape::Global => {
                let addr = format!("{}/inc", self.state.active_page.osc_path);
                self.send_osc(addr, vec![OscType::Float(delta * multiplier)]);
            }
        }
    }

    pub fn handle_encdr_event(&mut self, event: Event) {
        match event {
            Event::EncoderFine { name, delta, .. } => {
                if let Some(idx) = self.encoder_names.iter().position(|&n| n == name) {
                    if delta.abs() <= 0.7 {
                        self.apply_encoder_action(idx, delta as f32, true);
                        self.modifier_used_as_gesture = true;
                    }
                }
            }
            Event::Encoder { name, delta, .. } => {
                if let Some(idx) = self.encoder_names.iter().position(|&n| n == name) {
                    self.apply_encoder_action(idx, delta as f32, false);
                    self.modifier_used_as_gesture = true;
                }
            }
            Event::Button { name, pressed, .. } => {
                if name == "shift" {
                    self.state.shift_pressed = pressed;
                    let shift_val = if pressed { LedValue::Single(255) } else { LedValue::Single(0) };
                    self.encdr.set_led_in_group(self.device_id, "button_leds", "shift", shift_val);
                } else if name == "mute" {
                    if pressed {
                        self.state.active_mode = AppMode::Mute;
                        self.modifier_used_as_gesture = false;
                        self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", LedValue::Single(255));
                    } else {
                        if !self.modifier_used_as_gesture {
                            // Tapped: toggle latch
                            if self.state.latched_mode == AppMode::Mute {
                                self.state.latched_mode = AppMode::Normal;
                                self.state.active_mode = AppMode::Normal;
                                self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", LedValue::Single(0));
                            } else {
                                self.state.latched_mode = AppMode::Mute;
                                self.state.active_mode = AppMode::Mute;
                                self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", LedValue::Single(255));
                            }
                        } else {
                            // Held & used as gesture: revert to latched mode
                            self.state.active_mode = self.state.latched_mode.clone();
                            let led_val = if self.state.active_mode == AppMode::Mute { LedValue::Single(255) } else { LedValue::Single(0) };
                            self.encdr.set_led_in_group(self.device_id, "button_leds", "mute", led_val);
                        }
                    }
                } else if name == "solo" {
                    if pressed {
                        self.state.active_mode = AppMode::Solo;
                        self.modifier_used_as_gesture = false;
                        self.encdr.set_led_in_group(self.device_id, "button_leds", "solo", LedValue::Single(255));
                    } else {
                        if !self.modifier_used_as_gesture {
                            if self.state.latched_mode == AppMode::Solo {
                                self.state.latched_mode = AppMode::Normal;
                                self.state.active_mode = AppMode::Normal;
                                self.encdr.set_led_in_group(self.device_id, "button_leds", "solo", LedValue::Single(0));
                            } else {
                                self.state.latched_mode = AppMode::Solo;
                                self.state.active_mode = AppMode::Solo;
                                self.encdr.set_led_in_group(self.device_id, "button_leds", "solo", LedValue::Single(255));
                            }
                        } else {
                            self.state.active_mode = self.state.latched_mode.clone();
                            let led_val = if self.state.active_mode == AppMode::Solo { LedValue::Single(255) } else { LedValue::Single(0) };
                            self.encdr.set_led_in_group(self.device_id, "button_leds", "solo", led_val);
                        }
                    }
                } else if pressed {
                    self.modifier_used_as_gesture = true;
                    if name == "play" {
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
                        self.encdr.set_led_in_group(self.device_id, "button_leds", "restart", LedValue::Single(255));
                    } else if name == "arrow_left" {
                        self.send_osc("/device/nav/prev".to_string(), vec![]);
                    } else if name == "arrow_right" {
                        self.send_osc("/device/nav/next".to_string(), vec![]);
                    }
                } else if !pressed {
                    if name == "restart" {
                        self.encdr.set_led_in_group(self.device_id, "button_leds", "restart", LedValue::Single(0));
                    }
                }
            }
            _ => {}
        }
    }

    pub fn check_and_send_state(&mut self) {
        if self.state.dirty {
            let state_json = serde_json::json!({
                "device": self.state.device_name,
                "names": self.state.param_names,
                "displays": self.state.param_displays,
                "values": self.state.param_values,
                "modulated": self.state.param_modulated,
                "track_muted": self.state.track_muted,
            });
            let _ = self.state_tx.send(state_json);
            self.state.dirty = false;
        }
    }
}
