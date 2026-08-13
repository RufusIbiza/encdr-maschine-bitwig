// Load Bitwig API version 25
loadAPI(25);

// Define the controller with Vendor "Rufus", Name "Maschine Mk3 (OSC)", Version, UUID, and Author.
host.defineController("Rufus", "Maschine Mk3 (OSC)", "1.0", "f876359e-4b48-43e9-a3b0-0ab8548a31ec", "Rufus");
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

var MODULATION_POLL_MS = 40;

// State caches to avoid spamming the OSC connection with unchanged values
var lastValues = {};
var lastNames = {};

// Cached state populated by observers as Bitwig updates them
var cachedTrackMuted = 0;
var cachedIsPlaying = 0;
var cachedIsRecording = 0;
var cachedDeviceName = "No Device";
var cachedParamNames = ["", "", "", "", "", "", "", ""];
var cachedParamDisplays = ["", "", "", "", "", "", "", ""];
var cachedParamValues = [0, 0, 0, 0, 0, 0, 0, 0];
var cachedParamModulated = [0, 0, 0, 0, 0, 0, 0, 0];

// UDP ports for two-way communication between Bitwig and the Rust backend
var RUST_APP_PORT = 8081;
var BITWIG_PORT = 8082;

function init() {
    cursorTrack = host.createCursorTrack(1, 0);
    cursorDevice = cursorTrack.createCursorDevice();
    remoteControls = cursorDevice.createCursorRemoteControlsPage(8);
    transport = host.createTransport();

    var osc = host.getOscModule();
    var addressSpace = osc.createAddressSpace();
    
    addressSpace.registerMethod("/refresh", "", "Refresh state", function(source, msg) {
        handleRefresh();
    });

    addressSpace.registerDefaultMethod(function(source, msg) {
        var addr = msg.getAddressPattern();
        var args = msg.getArguments();
        
        if (addr.startsWith("/device/param/")) {
            var parts = addr.split("/");
            if (parts.length === 5 && parts[4] === "set") {
                var index = parseInt(parts[3], 10);
                if (index >= 0 && index < 8 && args.length > 0) {
                    var val = args[0];
                    remoteControls.getParameter(index).value().set(val);
                }
            } else if (parts.length === 5 && parts[4] === "inc") {
                var index = parseInt(parts[3], 10);
                if (index >= 0 && index < 8 && args.length > 0) {
                    var val = args[0];
                    remoteControls.getParameter(index).inc(val, 128);
                }
            }
        } else if (addr === "/device/nav/next") {
            cursorDevice.selectNext();
        } else if (addr === "/device/nav/prev") {
            cursorDevice.selectPrevious();
        } else if (addr === "/transport/play") {
            if (isCurrentlyPlaying) {
                transport.stop();
            } else {
                transport.play();
            }
        } else if (addr === "/transport/stop") {
            transport.stop();
        } else if (addr === "/transport/rec") {
            transport.isArrangerRecordEnabled().toggle();
        } else if (addr === "/transport/rec_count_in") {
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
        } else if (addr === "/refresh") {
            handleRefresh();
        }
    });

    oscServer = osc.createUdpServer(BITWIG_PORT, addressSpace);
    oscClient = osc.connectToUdpServer("127.0.0.1", RUST_APP_PORT, osc.createAddressSpace());

    // Observers
    cursorTrack.mute().markInterested();
    cursorTrack.mute().addValueObserver(function(isMuted) {
        cachedTrackMuted = isMuted ? 1 : 0;
        sendOsc("/track/mute", cachedTrackMuted);
    });

    transport.isPlaying().markInterested();
    transport.isPlaying().addValueObserver(function(isPlaying) {
        cachedIsPlaying = isPlaying ? 1 : 0;
        isCurrentlyPlaying = isPlaying;
        sendOsc("/transport/play", cachedIsPlaying);
    });

    transport.isArrangerRecordEnabled().markInterested();
    transport.isArrangerRecordEnabled().addValueObserver(function(isRecording) {
        cachedIsRecording = isRecording ? 1 : 0;
        sendOsc("/transport/rec", cachedIsRecording);
    });

    cursorDevice.name().markInterested();
    cursorDevice.name().addValueObserver(function(name) {
        cachedDeviceName = name || "No Device";
        lastNames = {};
        lastValues = {};
        sendOsc("/device/name", cachedDeviceName);
    });

    for (var i = 0; i < 8; i++) {
        var param = remoteControls.getParameter(i);
        params[i] = param;
        param.name().markInterested();
        param.value().markInterested();
        param.displayedValue().markInterested();
        param.modulatedValue().markInterested();
        
        (function(index) {
            param.name().addValueObserver(function(name) {
                cachedParamNames[index] = name || "";
                if (lastNames[index] !== name) {
                    lastNames[index] = name;
                    sendOsc("/device/param/" + index + "/name", cachedParamNames[index]);
                }
            });
            param.value().addValueObserver(function(value) {
                cachedParamValues[index] = typeof value === "number" ? value : 0.0;
                if (lastValues[index] !== value) {
                    lastValues[index] = value;
                    sendOsc("/device/param/" + index + "/value", cachedParamValues[index]);
                }
            });
            param.displayedValue().addValueObserver(function(displayStr) {
                cachedParamDisplays[index] = displayStr || "";
                sendOsc("/device/param/" + index + "/display", cachedParamDisplays[index]);
            });
        })(i);
    }
    
    pollModulation();
    println("Maschine Mk3 (OSC) initialized on port " + BITWIG_PORT);
}

function handleRefresh() {
    // Pull live values directly instead of replaying the cache: the cache is only
    // updated by observer change-callbacks, which may never fire again if the
    // device/track selection hasn't changed since Bitwig started - .get() always
    // reflects the true current state since these values were marked interested.
    sendOsc("/transport/play", transport.isPlaying().get() ? 1 : 0);
    sendOsc("/transport/rec", transport.isArrangerRecordEnabled().get() ? 1 : 0);
    sendOsc("/track/mute", cursorTrack.mute().get() ? 1 : 0);
    sendOsc("/device/name", cursorDevice.name().get() || "No Device");
    for (var i = 0; i < 8; i++) {
        var param = params[i];
        sendOsc("/device/param/" + i + "/name", param.name().get() || "");
        sendOsc("/device/param/" + i + "/display", param.displayedValue().get() || "");
        sendOsc("/device/param/" + i + "/value", param.value().get());
        sendOsc("/device/param/" + i + "/modulated", param.modulatedValue().get());
    }
}

function pollModulation() {
    for (var i = 0; i < 8; i++) {
        var modValue = params[i].modulatedValue().get();
        if (typeof modValue === "number") {
            cachedParamModulated[i] = modValue;
            if (modValue !== lastModulatedValues[i]) {
                lastModulatedValues[i] = modValue;
                sendOsc("/device/param/" + i + "/modulated", modValue);
            }
        }
    }
    host.scheduleTask(pollModulation, MODULATION_POLL_MS);
}

function sendOsc(address, arg) {
    if (oscClient) {
        try {
            if (typeof arg === "string") {
                oscClient.sendMessage(address, arg);
            } else if (typeof arg === "number") {
                oscClient.sendMessage(address, arg);
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
