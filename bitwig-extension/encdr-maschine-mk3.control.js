// Load Bitwig API version 25
loadAPI(25);

// Define the controller with Vendor "Rufus", Name "Maschine Mk3 (OSC)", Version, UUID, and Author.
// Changing the vendor name here organizes it under "Rufus" in the Bitwig controller settings.
host.defineController("Rufus", "Maschine Mk3 (OSC)", "1.0", "f876359e-4b48-43e9-a3b0-0ab8548a31ec", "Rufus");
// Define dummy MIDI ports. The script relies on OSC instead of standard MIDI, 
// but Bitwig requires at least 1 input and output port to initialize a controller properly.
host.defineMidiPorts(1, 1);

// Global references to Bitwig API objects
var oscServer;
var oscClient;
var cursorTrack;
var cursorDevice;
var remoteControls;
var transport;
var isCurrentlyPlaying = false;
var params = [];

// How often (in milliseconds) we poll for high-rate modulation values. 
// This is necessary because Bitwig's JS API does not always fire observer callbacks 
// for audio-rate modulations fast enough.
var MODULATION_POLL_MS = 40;

// State caches to avoid spamming the OSC connection with unchanged values
var lastValues = {};
var lastNames = {};

// UDP ports for two-way communication between Bitwig and the Rust backend
var RUST_APP_PORT = 8081;
var BITWIG_PORT = 8082;

function init() {
    // 1. Setup Bitwig API objects FIRST
    // Create cursors that follow the user's selected track and device.
    cursorTrack = host.createCursorTrack(1, 0);
    cursorDevice = cursorTrack.createCursorDevice();
    // Grab the first 8 remote control parameters of the currently selected device.
    remoteControls = cursorDevice.createCursorRemoteControlsPage(8);
    // Create a transport object to control playback, recording, and looping.
    transport = host.createTransport();

    // 2. Initialize OSC communication
    var osc = host.getOscModule();
    var addressSpace = osc.createAddressSpace();
    
    // Register a catch-all listener for incoming OSC messages from the Rust backend.
    addressSpace.registerDefaultMethod(function(source, msg) {
        var addr = msg.getAddressPattern();
        var args = msg.getArguments();
        
        if (addr.startsWith("/device/param/")) {
            var parts = addr.split("/");
            // Handle absolute value setting
            if (parts.length === 5 && parts[4] === "set") {
                var index = parseInt(parts[3], 10);
                if (index >= 0 && index < 8 && args.length > 0) {
                    var val = args[0];
                    remoteControls.getParameter(index).value().set(val);
                }
            // Handle incremental value changes (e.g., from hardware encoders)
            } else if (parts.length === 5 && parts[4] === "inc") {
                var index = parseInt(parts[3], 10);
                if (index >= 0 && index < 8 && args.length > 0) {
                    var val = args[0];
                    // Map the delta to the parameter. Resolution of 128 means a val of 1 increments by 1/128th.
                    remoteControls.getParameter(index).inc(val, 128);
                }
            }
        // Device Navigation
        } else if (addr === "/device/nav/next") {
            cursorDevice.selectNext();
        } else if (addr === "/device/nav/prev") {
            cursorDevice.selectPrevious();
        // Transport Controls
        } else if (addr === "/transport/play") {
            transport.play();
        } else if (addr === "/transport/stop") {
            transport.stop();
        } else if (addr === "/transport/rec") {
            transport.isArrangerRecordEnabled().toggle();
        } else if (addr === "/transport/rec_count_in") {
            // Special handling for count-in recording
            if (typeof transport.preRoll === "function") {
                try { transport.preRoll().set("1 bar"); } catch(e1) {}
            }
            if (typeof transport.isPreRollEnabled === "function") {
                try { transport.isPreRollEnabled().set(true); } catch(e2) {}
            }
            transport.isArrangerRecordEnabled().set(true);
            if (!isCurrentlyPlaying) {
                transport.play();
            }
        } else if (addr === "/transport/restart") {
            // Restart playback or return to 0 depending on the current play state
            if (isCurrentlyPlaying) {
                if (typeof transport.jumpToPlaystartPosition === "function") {
                    try { transport.jumpToPlaystartPosition(); } catch(e) { transport.restart(); }
                } else {
                    transport.restart();
                }
                transport.play();
            } else {
                transport.getPosition().set(0);
                transport.play();
            }
        } else if (addr === "/transport/loop_toggle") {
            transport.isArrangerLoopEnabled().toggle();
        }
    });

    // Start the OSC Server to receive messages and the Client to send messages.
    oscServer = osc.createUdpServer(BITWIG_PORT, addressSpace);
    oscClient = osc.connectToUdpServer("127.0.0.1", RUST_APP_PORT, osc.createAddressSpace());

    // 3. Setup Observers to sync Bitwig state to the Rust application
    
    // Transport State Observers
    transport.isPlaying().markInterested();
    transport.isPlaying().addValueObserver(function(isPlaying) {
        isCurrentlyPlaying = isPlaying;
        sendOsc("/transport/play", isPlaying ? 1 : 0);
    });

    transport.isArrangerRecordEnabled().markInterested();
    transport.isArrangerRecordEnabled().addValueObserver(function(isRecording) {
        sendOsc("/transport/rec", isRecording ? 1 : 0);
    });

    // Device Observer
    cursorDevice.name().markInterested();
    cursorDevice.name().addValueObserver(function(name) {
        // Clear caches when device changes to ensure all parameters are resent
        lastNames = {};
        lastValues = {};
        sendOsc("/device/name", name);
    });

    // Setup observers for the 8 remote control parameters (Macros)
    for (var i = 0; i < 8; i++) {
        var param = remoteControls.getParameter(i);
        params[i] = param;
        param.name().markInterested();
        param.value().markInterested();
        
        // Use an IIFE (Immediately Invoked Function Expression) to capture the index properly in loops
        (function(index) {
            param.name().addValueObserver(function(name) {
                if (lastNames[index] !== name) {
                    lastNames[index] = name;
                    sendOsc("/device/param/" + index + "/name", name);
                }
            });
            param.value().addValueObserver(function(value) {
                if (lastValues[index] !== value) {
                    lastValues[index] = value;
                    sendOsc("/device/param/" + index + "/value", value);
                }
            });
            // modulatedValue() does not reliably push observer callbacks at modulation rate
            // (Bitwig only guarantees .get() is fresh when polled) - so this is a cheap bonus
            // path only. The MODULATION_POLL_MS task below is the mechanism we actually rely on.
            param.modulatedValue().markInterested();
            
            // Also forward the formatted string representation (e.g. "12.5 dB")
            param.displayedValue().addValueObserver(function(displayStr) {
                sendOsc("/device/param/" + index + "/display", displayStr);
            });
        })(i);
    }
    
    pollModulation();

    println("Maschine Mk3 (OSC) initialized on port " + BITWIG_PORT);
}

// Explicit timer, independent of Bitwig's observer/flush event system - modulatedValue()
// changes at modulation rate (e.g. an LFO) and Bitwig does not push a JS callback for every
// such change, so this is the only mechanism that reliably keeps the tick live.
function pollModulation() {
    for (var i = 0; i < 8; i++) {
        var modValue = params[i].modulatedValue().get();
        if (modValue !== lastModulatedValues[i]) {
            lastModulatedValues[i] = modValue;
            sendOsc("/device/param/" + i + "/modulated", modValue);
        }
    }
    host.scheduleTask(pollModulation, MODULATION_POLL_MS);
}

// Send OSC utility: safely wraps oscClient to prevent crashes on sending errors
function sendOsc(address, arg) {
    if (oscClient) {
        try {
            if (typeof arg === "string") {
                oscClient.sendMessage(address, arg);
            } else if (typeof arg === "number") {
                oscClient.sendMessage(address, arg); // Sends as float or int depending on Bitwig API wrapping
            }
        } catch (e) {
            println("OSC Send Error: " + e);
        }
    }
}

var lastModulatedValues = [];
for (var i = 0; i < 8; i++) {
    lastModulatedValues[i] = -1;
}

function flush() {
}

function exit() {
}
