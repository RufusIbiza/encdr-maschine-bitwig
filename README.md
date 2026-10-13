# Embit (Community Edition)

**Open-source, dual-display Maschine Mk3 controller for Bitwig Studio**

Formerly known as `encdr-maschine-bitwig`, **Embit** brings Native Instruments Maschine Mk3 hardware into Bitwig Studio, utilizing direct USB HID communication via [`encdr`](https://github.com/RufusIbiza/Encdr) and Bitwig's official JavaScript Controller API to deliver rich, full-color feedback on the Mk3's dual screens.

<p align="center">
  <img src="assets/left_screen.png" width="45%" alt="Left Screen">
  <img src="assets/right_screen.png" width="45%" alt="Right Screen">
</p>

---

> [!NOTE]
> **Looking for advanced features like Arranger Bird's-Eye View and Live Clip Rendering?**  
> Check out **[Embit Pro](https://github.com/RufusIbiza/embit-pro)**, the commercial edition featuring deep engine introspection via Java extension, live timeline clip rendering, sub-beat scrubbing, and expanded hardware navigation.

---

## Architectural Approach (Rust + OSC)

Historically, controller extensions (like the excellent DrivenByMoss) have run entirely inside Bitwig's Java sandbox. While powerful, the Java sandbox imposes limitations on direct hardware access, especially for high-resolution tasks like rendering UI on external displays.

**Embit** uses a decoupled architecture:
1. **Bitwig Controller Script (`encdr-maschine-mk3.control.js`)**: A lightweight script running inside Bitwig. It watches for changes in Bitwig's state (track selection, parameters, transport) and broadcasts them over UDP using Open Sound Control (OSC), while receiving incoming hardware commands from the daemon.
2. **Rust Backend Engine (`embit`)**: A standalone, high-performance Rust application that communicates directly with the Maschine hardware via USB HID using the [`encdr`](https://github.com/RufusIbiza/Encdr) library. It acts as an OSC server/client, rendering beautiful WebKit-based UIs to the Maschine screens with near-zero latency.

By moving the heavy lifting (like rendering screen UIs) out of the Bitwig sandbox and into a native Rust application, we unlock the full potential of the Maschine Mk3's dual displays.

## Current Features

### Core Infrastructure & Architecture
- **Bi-directional OSC Communication**: Seamlessly syncs state between Bitwig Studio and the Maschine Mk3 hardware over UDP with near-zero latency.
- **Svelte 5 Single Page Application (SPA) Displays**:
  - Left and right 480x272 screens render Bitwig-styled, dark-mode UIs using `encdr-view` and Svelte 5 runes (`$state`, `$derived`).
  - High performance, reactive SVG and HTML rendering with zero unnecessary overhead.
  - Inlined standalone builds for zero runtime web-server dependencies.

### Device Control (`PLUGIN` Mode)
- **Dynamic Dual Screens**:
  - Displays currently selected device name and parameters.
  - Displays names, values, and real-time modulated indicators (white tick tracks modulated values like LFOs and envelopes).
- **Hardware Controls**:
  - 8 screen encoders control the 8 macros/parameters with fine resolution support.
  - `LEFT` and `RIGHT` arrow buttons navigate through devices on the active track.

### Mixer & Track Control (`MIXER` & `CHANNEL` Modes)
- **8-Track Bank View (`MIXER` Mode)**:
  - Left Screen: Tracks 1 – 4; Right Screen: Tracks 5 – 8.
  - Track color headers, track numbers, and track names.
  - Real-time vertical stereo VU peak level meters with multi-segment color gradients.
  - Vertical volume faders, dB numerical readouts, and Pan indicators.
  - Status flags for Mute (`[M]`), Solo (`[S]`), Rec-Arm (`[●]`), and selection glow.
  - **Encoders 1 – 8**: Control Volume for tracks 1 – 8 (or Pan when holding `SHIFT`).
  - **Top Buttons (`top_1` to `top_8`)**: Select tracks, or toggle Mute/Solo/Arm when combined with `MUTE`, `SOLO`, or `SHIFT`.
  - **4-D Encoder & Arrows**: Bank tracks in increments of 8 channels (`◄` / `►`) or scroll tracks.
- **Focused Channel View (`CHANNEL` Mode)**:
  - Left Screen: Detailed view of the selected cursor track with master fader, large VU meters, pan knob, and track status.
  - Right Screen: 6 Send level controls (Send A through F) with dials and dB readouts.
  - **Encoders 1 – 8**: Encoders 1 & 2 control Volume and Pan; Encoders 3 – 8 control Sends 1 – 6.
- **Mode Switching**:
  - Press `MIXER` to enter 8-track mixer view.
  - Press `PLUGIN` to enter Device parameter view.
  - Press `CHANNEL` to enter Focused track channel view.
  - Press the 4-D encoder to toggle between Mixer and Device views.
  - Hardware button LEDs reflect the active mode.

### Transport Controls
- **PLAY**: Starts playback from current cuepoint. Reflects live playback status.
- **STOP**: Stops playback. Reflects stopped status.
- **REC**: Toggles arranger recording from playmarker. Reflects live record status.
- **SHIFT + REC**: Turns on count-in recording.
- **RESTART**: Returns to current cuepoint and continues playback if playing; sets playhead to 0 and plays if stopped.
- **SHIFT + RESTART**: Toggles arranger looping on/off.

---

## Setup & Installation Instructions

**Requirements:**
- Bitwig Studio 5.1 or newer (Uses Controller API version 25).
- Rust (`cargo`) installed on your system.
- The **Encdr** repository cloned as a sibling directory to this project (`../Encdr`).

### OS-Specific Considerations

#### Linux
To allow `encdr` to communicate with the USB hardware without requiring `sudo`, configure `udev` rules. 
Create `/etc/udev/rules.d/99-maschine.rules`:
```udev
SUBSYSTEM=="usb", ATTRS{idVendor}=="17cc", MODE="0666"
```
Then reload:
```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

Install associated webview dependencies:
```bash
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libcairo2-dev
```

#### Windows / macOS
Native Instruments background services (e.g., `NIHardwareHelper.exe`, `NIHostIntegrationAgent.exe`) may claim the USB interface. Terminate or disable these processes before launching Embit.

---

### Step-by-Step Installation

#### 1. Install the Bitwig Extension Script
Copy `bitwig-extension/encdr-maschine-mk3.control.js` into your Bitwig Controller Scripts directory:

*Linux / macOS:*
```bash
cp bitwig-extension/encdr-maschine-mk3.control.js "$HOME/Bitwig Studio/Controller Scripts/"
```

*Windows:*
`%USERPROFILE%\Documents\Bitwig Studio\Controller Scripts\`

#### 2. Enable Controller in Bitwig
1. Open Bitwig Studio.
2. Go to **Settings > Controllers**.
3. Click **Add controller manually**.
4. Select **Rufus** -> **Maschine Mk3 (OSC)**.
5. Set MIDI in/out ports to any virtual/dummy port (communication is handled via OSC UDP).

#### 3. Run the Rust Backend
```bash
cargo run --release
```

#### Desktop Screen Preview
To mirror dual screens in a desktop window:
```bash
cargo run --release -- --visible
# or:
cargo preview
```

---

## License

Embit (Community Edition) is licensed under the [GNU General Public License v3.0](LICENSE).
