use std::net::UdpSocket;
use std::time::Duration;

use encdr::{Encdr, EncdrConfig, Event, LedValue};
use encdr_view::{ScreenContent, ScreenView};
use rosc::{OscPacket, OscType, encoder, decoder, OscMessage};

fn main() {

    // Initialize the Encdr library, which manages HID communication with the Maschine hardware
    let mut encdr = Encdr::new(EncdrConfig::default()).expect("Failed to init encdr");
    
    // Scan USB buses for any connected Maschine Mk3 devices
    let ids = encdr.scan().expect("Scan failed");
    if ids.is_empty() {
        eprintln!("No Maschine Mk3 found!");
        return;
    }
    
    // Connect to the first discovered device and retrieve its capabilities
    let device_id = ids[0];
    let desc = encdr.device_descriptor(device_id).unwrap().clone();
    println!("Connected to {}", desc.name);

    std::thread::sleep(Duration::from_millis(500));

    // Initialize all black button LEDs to OFF (0) by default
    for group in &desc.leds {
        if group.id == "button_leds" {
            for item in &group.items {
                encdr.set_led_in_group(device_id, &group.id, item.name(), LedValue::Single(0));
            }
        }
    }
    // Set stop LED to ON (255) by default as transport starts stopped
    encdr.set_led_in_group(device_id, "button_leds", "stop", LedValue::Single(255));

    // Setup HTML screens for left and right displays
    // We embed the HTML content directly into the binary using include_str! macro
    let left_html = include_str!("../screens/left.html");
    let right_html = include_str!("../screens/right.html");

    // Initialize the left WebKit view. The `false` flag indicates it's not headless.
    let left_view = ScreenView::new(
        &encdr, device_id, "left",
        ScreenContent::Html(left_html.to_string()), false,
    ).expect("Left WebView failed");

    // Initialize the right WebKit view
    let right_view = ScreenView::new(
        &encdr, device_id, "right",
        ScreenContent::Html(right_html.to_string()), false,
    ).expect("Right WebView failed");

    // Bind UDP socket for receiving OSC messages from Bitwig (listening on 8081)
    let socket = UdpSocket::bind("127.0.0.1:8081").unwrap();
    // Set to non-blocking so our main event loop doesn't stall waiting for network packets
    socket.set_nonblocking(true).unwrap();

    // Get a reference to the hardware events channel receiver
    let events = encdr.events().clone();
    let mut frame_count = 0u64;

    let encoder_names = [
        "screen_encoder_1", "screen_encoder_2", "screen_encoder_3", "screen_encoder_4",
        "screen_encoder_5", "screen_encoder_6", "screen_encoder_7", "screen_encoder_8",
    ];

    // Application state caches corresponding to Bitwig's current device and parameters
    let mut device_name = String::new();
    let mut param_names: [String; 8] = std::array::from_fn(|_| String::new());
    let mut param_displays: [String; 8] = std::array::from_fn(|_| String::new());
    let mut param_values: [f32; 8] = [0.0; 8];
    let mut param_modulated: [f32; 8] = [0.0; 8];

    // Flags to orchestrate rendering and event capturing across iterations
    let mut dirty = true;
    let mut needs_capture = false;
    let mut shift_pressed = false;

    println!("Listening for OSC on port 8081, sending to 8082...");

    loop {
        ScreenView::pump_events();

        // 1. Process OSC messages from Bitwig
        // We receive UDP packets and decode them into native Rust structs using the `rosc` library
        let mut buf = [0u8; 8192];
        while let Ok((size, _addr)) = socket.recv_from(&mut buf) {
            if let Ok((_, OscPacket::Message(msg))) = decoder::decode_udp(&buf[..size]) {
                if msg.addr == "/device/name" {
                    if let Some(OscType::String(name)) = msg.args.get(0) {
                        device_name = name.clone();
                        dirty = true;
                    }
                } else if msg.addr == "/transport/play" {
                    let active = match msg.args.get(0) {
                        Some(OscType::Int(v)) => *v != 0,
                        Some(OscType::Float(v)) => *v != 0.0,
                        Some(OscType::Bool(v)) => *v,
                        _ => false,
                    };
                    let play_val = if active { LedValue::Single(255) } else { LedValue::Single(0) };
                    let stop_val = if active { LedValue::Single(0) } else { LedValue::Single(255) };
                    encdr.set_led_in_group(device_id, "button_leds", "play", play_val);
                    encdr.set_led_in_group(device_id, "button_leds", "stop", stop_val);
                } else if msg.addr == "/transport/rec" {
                    let active = match msg.args.get(0) {
                        Some(OscType::Int(v)) => *v != 0,
                        Some(OscType::Float(v)) => *v != 0.0,
                        Some(OscType::Bool(v)) => *v,
                        _ => false,
                    };
                    let rec_val = if active { LedValue::Single(255) } else { LedValue::Single(0) };
                    encdr.set_led_in_group(device_id, "button_leds", "rec", rec_val);
                } else if msg.addr.starts_with("/device/param/") {
                    let parts: Vec<&str> = msg.addr.split('/').collect();
                    if parts.len() == 5 {
                        if let Ok(idx) = parts[3].parse::<usize>() {
                            if idx < 8 {
                                if parts[4] == "name" {
                                    if let Some(OscType::String(val)) = msg.args.get(0) {
                                        param_names[idx] = val.clone();
                                        dirty = true;
                                    }
                                } else if parts[4] == "display" {
                                    if let Some(OscType::String(val)) = msg.args.get(0) {
                                        param_displays[idx] = val.clone();
                                        dirty = true;
                                    }
                                } else if parts[4] == "value" {
                                    if let Some(OscType::Float(val)) = msg.args.get(0) {
                                        param_values[idx] = *val;
                                        dirty = true;
                                    } else if let Some(OscType::Double(val)) = msg.args.get(0) {
                                        param_values[idx] = *val as f32;
                                        dirty = true;
                                    }
                                } else if parts[4] == "modulated" {
                                    if let Some(OscType::Float(val)) = msg.args.get(0) {
                                        param_modulated[idx] = *val;
                                        dirty = true;
                                    } else if let Some(OscType::Double(val)) = msg.args.get(0) {
                                        param_modulated[idx] = *val as f32;
                                        dirty = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Process Hardware Events
        while let Ok(event) = events.try_recv() {
            match event {
                Event::EncoderFine { name, delta, .. } => {
                    if let Some(idx) = encoder_names.iter().position(|&n| n == name) {
                        // The hardware reports absolute values that wrap around (e.g., 0 to 1023). 
                        // Encdr converts this to a delta, but doesn't filter the wrap-around jump.
                        // Based on openAV-Ctlra (maschine_mk3.c:724), we must drop massive jumps (> 0.7).
                        if delta.abs() > 0.7 {
                            continue;
                        }
                        
                        // We must also boost the signal so Bitwig registers the movement natively.
                        // Bitwig's `inc(x, 128)` moves x/128 steps. A physical tick produces ~0.01 delta.
                        // Multiplying by 100.0 means 1 tick = ~1/128 step, which is perfect for endless encoders.
                        let msg = encoder::encode(&OscPacket::Message(OscMessage {
                            addr: format!("/device/param/{}/inc", idx),
                            args: vec![OscType::Float(delta as f32 * 100.0)],
                        })).unwrap();
                        let _ = socket.send_to(&msg, "127.0.0.1:8082");
                    }
                }
                Event::Encoder { name, delta, .. } => {
                    if let Some(idx) = encoder_names.iter().position(|&n| n == name) {
                        let msg = encoder::encode(&OscPacket::Message(OscMessage {
                            addr: format!("/device/param/{}/inc", idx),
                            args: vec![OscType::Float(delta as f32 * 10.0)], // Coarse movements should step faster
                        })).unwrap();
                        let _ = socket.send_to(&msg, "127.0.0.1:8082");
                    }
                }
                Event::Button { name, pressed, .. } => {
                    if name == "shift" {
                        shift_pressed = pressed;
                        let shift_val = if pressed { LedValue::Single(255) } else { LedValue::Single(0) };
                        encdr.set_led_in_group(device_id, "button_leds", "shift", shift_val);
                    } else if pressed {
                        if name == "play" {
                            let msg = encoder::encode(&OscPacket::Message(OscMessage {
                                addr: "/transport/play".to_string(), args: vec![],
                            })).unwrap();
                            let _ = socket.send_to(&msg, "127.0.0.1:8082");
                        } else if name == "stop" {
                            let msg = encoder::encode(&OscPacket::Message(OscMessage {
                                addr: "/transport/stop".to_string(), args: vec![],
                            })).unwrap();
                            let _ = socket.send_to(&msg, "127.0.0.1:8082");
                        } else if name == "rec" {
                            let addr = if shift_pressed {
                                "/transport/rec_count_in".to_string()
                            } else {
                                "/transport/rec".to_string()
                            };
                            let msg = encoder::encode(&OscPacket::Message(OscMessage {
                                addr, args: vec![],
                            })).unwrap();
                            let _ = socket.send_to(&msg, "127.0.0.1:8082");
                        } else if name == "restart" {
                            let addr = if shift_pressed {
                                "/transport/loop_toggle".to_string()
                            } else {
                                "/transport/restart".to_string()
                            };
                            let msg = encoder::encode(&OscPacket::Message(OscMessage {
                                addr, args: vec![],
                            })).unwrap();
                            let _ = socket.send_to(&msg, "127.0.0.1:8082");
                            encdr.set_led_in_group(device_id, "button_leds", "restart", LedValue::Single(255));
                        } else if name == "arrow_left" {
                            let msg = encoder::encode(&OscPacket::Message(OscMessage {
                                addr: "/device/nav/prev".to_string(), args: vec![],
                            })).unwrap();
                            let _ = socket.send_to(&msg, "127.0.0.1:8082");
                        } else if name == "arrow_right" {
                            let msg = encoder::encode(&OscPacket::Message(OscMessage {
                                addr: "/device/nav/next".to_string(), args: vec![],
                            })).unwrap();
                            let _ = socket.send_to(&msg, "127.0.0.1:8082");
                        }
                    } else if !pressed {
                        if name == "restart" {
                            encdr.set_led_in_group(device_id, "button_leds", "restart", LedValue::Single(0));
                        }
                    }
                }
                _ => {}
            }
        }

        // 3. Update the WebView screens if any state changed
        if dirty {
            // Serialize the current state to JSON to send it to the embedded JS context inside the web views
            let state = serde_json::json!({
                "device": device_name,
                "names": param_names,
                "displays": param_displays,
                "values": param_values,
                "modulated": param_modulated,
            });
            left_view.send("state", state.clone());
            right_view.send("state", state.clone());
            dirty = false;
            // Mark that the DOM has changed and we need to capture a new frame
            needs_capture = true;
        }

        left_view.poll(&encdr);
        right_view.poll(&encdr);

        // 4. Capture rendered frames and send them to the hardware displays
        if needs_capture {
            ScreenView::pump_events();
            // Capture the raw pixel buffer from WebKit and submit it over USB/HID
            let _ = left_view.capture_and_submit(&encdr);
            let _ = right_view.capture_and_submit(&encdr);
            needs_capture = false;
        }

        frame_count += 1;
        // Periodically force a frame submission (approx. once per second at an 8ms polling interval) 
        // This ensures the screens don't permanently drop out or freeze if a capture event is missed
        if frame_count % 125 == 0 {
            let _ = left_view.capture_and_submit(&encdr);
            let _ = right_view.capture_and_submit(&encdr);
        }

        std::thread::sleep(Duration::from_millis(8));
    }
}
