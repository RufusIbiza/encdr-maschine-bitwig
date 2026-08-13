mod logic;

use std::net::UdpSocket;
use std::time::Duration;
use std::sync::Arc;

use encdr::{Encdr, EncdrConfig, LedValue};
use encdr_view::{ScreenContent, ScreenView};
use rosc::{OscPacket, decoder, OscMessage};
use crossbeam_channel::{unbounded, select};

use logic::AppLogic;

fn main() {
    // Initialize the Encdr library
    let mut encdr = Encdr::new(EncdrConfig::default()).expect("Failed to init encdr");
    
    let ids = encdr.scan().expect("Scan failed");
    if ids.is_empty() {
        eprintln!("No Maschine Mk3 found!");
        return;
    }
    
    let encdr = Arc::new(encdr);
    
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
    encdr.set_led_in_group(device_id, "button_leds", "stop", LedValue::Single(255));

    let left_html = include_str!("../screens/left.html");
    let right_html = include_str!("../screens/right.html");

    let left_view = ScreenView::new(
        &encdr, device_id, "left",
        ScreenContent::Html(left_html.to_string()), false,
    ).expect("Left WebView failed");

    let right_view = ScreenView::new(
        &encdr, device_id, "right",
        ScreenContent::Html(right_html.to_string()), false,
    ).expect("Right WebView failed");

    let socket = UdpSocket::bind("127.0.0.1:8081").unwrap();
    socket.set_nonblocking(false).unwrap();

    let encdr_events = encdr.events().clone();
    let (osc_tx, osc_rx) = unbounded::<OscMessage>();
    let (state_tx, state_rx) = unbounded::<serde_json::Value>();

    // Thread 1: OSC Receiver
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok((size, _addr)) = socket.recv_from(&mut buf) {
            if let Ok((_, OscPacket::Message(msg))) = decoder::decode_udp(&buf[..size]) {
                let _ = osc_tx.send(msg);
            }
        }
    });

    // Thread 2: Logic and State Machine
    let encdr_clone = Arc::clone(&encdr);
    let ticker = crossbeam_channel::tick(Duration::from_millis(500));
    std::thread::spawn(move || {
        let mut app_logic = AppLogic::new(state_tx, encdr_clone, device_id);
        
        loop {
            select! {
                recv(osc_rx) -> msg => {
                    if let Ok(m) = msg {
                        app_logic.handle_osc_message(m);
                        app_logic.check_and_send_state();
                    }
                }
                recv(encdr_events) -> event => {
                    if let Ok(e) = event {
                        app_logic.handle_encdr_event(e);
                        app_logic.check_and_send_state();
                    }
                }
                recv(ticker) -> _ => {
                    if app_logic.state.device_name.is_empty() {
                        app_logic.send_osc("/refresh".to_string(), vec![]);
                    }
                }
            }
        }
    });

    println!("Listening for OSC on port 8081, sending to 8082...");

    let mut frame_count = 0u64;
    let mut needs_capture = false;
    let mut current_state: Option<serde_json::Value> = None;
    let mut last_capture = std::time::Instant::now();
    let min_capture_interval = Duration::from_millis(40); // Cap hardware screen updates at ~25 FPS

    // Thread 3 (Main): Render Loop
    loop {
        ScreenView::pump_events();

        // 1. Drain pending state updates from the logic thread
        let mut state_updated = false;
        while let Ok(state) = state_rx.try_recv() {
            current_state = Some(state);
            state_updated = true;
        }

        // 2. Only push to the WebViews (and thus trigger a screenshot) when state actually changed
        if state_updated {
            if let Some(ref state) = current_state {
                left_view.send("state", state.clone());
                right_view.send("state", state.clone());
                needs_capture = true;
            }
        }

        // 3. Capture rendered frames and submit to hardware (rate-limited to 25 FPS max)
        if needs_capture && last_capture.elapsed() >= min_capture_interval {
            ScreenView::pump_events();
            let _ = left_view.capture_and_submit(&encdr);
            let _ = right_view.capture_and_submit(&encdr);
            needs_capture = false;
            last_capture = std::time::Instant::now();
        }

        // Heartbeat capture (~0.2Hz) to recover from any dropped USB frames, independent of state changes
        frame_count += 1;
        if frame_count % 300 == 0 {
            let _ = left_view.capture_and_submit(&encdr);
            let _ = right_view.capture_and_submit(&encdr);
        }

        std::thread::sleep(Duration::from_millis(16));
    }
}
