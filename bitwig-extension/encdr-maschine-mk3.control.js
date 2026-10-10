// Bitwig Studio Controller Script for Maschine Mk3 (OSC Bridge)
// API Version 25
loadAPI(25);

host.defineController("Rufus", "Maschine Mk3 (OSC)", "3.0", "f876359e-4b48-43e9-a3b0-0ab8548a31ec", "Rufus");
host.defineMidiPorts(1, 1);

// Global references to Bitwig API objects
var oscServer;
var oscClient;
var cursorTrack;
var cursorDevice;
var remoteControls;
var trackBank;
var transport;
var isCurrentlyPlaying = false;
var params = [];
var bankTracks = [];
var cursorSends = [];
var masterTrack;
var isViewingMaster = false;
var lastNonMasterTrackName = "";
var cachedMonitorMode = "AUTO";
var cachedIsGroup = 0;

var TICK_RATE_MS = 40;

function debugLog(msg) {
    try {
        var fw = new java.io.FileWriter("/tmp/bitwig_script.log", true);
        var bw = new java.io.BufferedWriter(fw);
        bw.write(new java.util.Date() + " - " + msg + "\n");
        bw.close();
    } catch(e) {}
    try {
        println("[Maschine OSC] " + msg);
    } catch(e) {}
}

debugLog("Script encdr-maschine-mk3.control.js evaluated");

// Cached State
var cachedDeviceName = "No Device";
var cachedTrackMuted = 0;
var cachedIsPlaying = 0;
var cachedIsRecording = 0;

var cachedBankOffset = 0;
var cachedBankCanScrollBack = 0;
var cachedBankCanScrollForw = 0;

var cachedBankColors = [
    "#ff8c1a", "#ff8c1a", "#ff8c1a", "#ff8c1a",
    "#ff8c1a", "#ff8c1a", "#ff8c1a", "#ff8c1a",
    "#ff8c1a", "#ff8c1a", "#ff8c1a", "#ff8c1a",
    "#ff8c1a", "#ff8c1a", "#ff8c1a", "#ff8c1a"
];
var cachedCursorColor = "#ff8c1a";

var returnTrack;
var bankSelected = [];
var bankParentTracks = [];
var cachedParentExists = [];
var cachedParentColors = [];
var cachedParentNames = [];
var cachedGrandparentColors = [];
var cachedIsGroupExpanded = [];
for (var gi = 0; gi < 32; gi++) {
    bankSelected[gi] = false;
    bankParentTracks[gi] = null;
    cachedParentExists[gi] = 0;
    cachedParentColors[gi] = "";
    cachedParentNames[gi] = "";
    cachedGrandparentColors[gi] = "";
    cachedIsGroupExpanded[gi] = 0;
}
var cachedCursorGroupExpanded = 0;

// Nesting depth of each bank track. Bitwig's API has no depth attribute and its parent-track
// proxies only ever point at the group itself, so depth is rebuilt from the flat bank: each
// group's own child bank reports how many direct children it has, and an open group's
// children follow it directly in the main bank (a closed group's children are left out).
var cachedDepth = [];
var cachedChildCount = [];
var cachedActivated = [];
var depthDirty = true;
for (var di = 0; di < 32; di++) {
    cachedDepth[di] = 0;
    cachedChildCount[di] = 0;
    cachedActivated[di] = true;
}

function recomputeDepths() {
    depthDirty = false;
    var stack = []; // children still expected for each open group, outermost first
    for (var t = 0; t < 32; t++) {
        var trk = bankTracks[t];
        var d = 0;
        if (trk && trk.exists().get()) {
            while (stack.length && stack[stack.length - 1] <= 0) stack.pop();
            d = stack.length;
            if (stack.length) stack[stack.length - 1]--;
            if (trk.isGroup().get() && cachedIsGroupExpanded[t] && cachedChildCount[t] > 0) {
                stack.push(cachedChildCount[t]);
            }
        }
        if (d !== cachedDepth[t]) {
            cachedDepth[t] = d;
            sendOsc("/track/bank/" + t + "/depth", d);
        }
    }
}

// Channel strip bank: one large, unpaged view of the whole (visible) track list, watched only
// for what the strip and 4D-encoder navigation need. It is separate from the 32-track bank the
// mixer pages through, so the strip always knows every channel, not just a window of them.
// Bitwig banks have a fixed size; if a project ever lists more tracks than this, the bank
// follows the cursor track and the strip falls back to showing a window again.
var STRIP_SIZE = 256;
var stripBank;
var stripTracks = [];
var stripColors = [];
var stripChildCount = [];
var stripSelected = [];
var stripDepth = [];
var stripDepthDirty = true;
var stripOffset = 0;
for (var sti = 0; sti < STRIP_SIZE; sti++) {
    stripColors[sti] = "#888892";
    stripChildCount[sti] = 0;
    stripSelected[sti] = false;
    stripDepth[sti] = 0;
}

function stripExists(i) {
    var trk = stripTracks[i];
    return !!(trk && trk.exists().get());
}

function stripIsActivated(i) {
    try { return stripTracks[i].isActivated().get(); } catch(e) { return true; }
}

// Sends every field of strip slot `i` (used when a track appears in the slot, and on refresh).
function sendStripSlot(i) {
    var trk = stripTracks[i];
    if (!trk) return;
    if (!trk.exists().get()) {
        sendOsc("/strip/" + i + "/exists", 0);
        return;
    }
    sendOsc("/strip/" + i + "/name", trk.name().get() || "");
    sendOsc("/strip/" + i + "/color", stripColors[i]);
    sendOsc("/strip/" + i + "/is_group", trk.isGroup().get() ? 1 : 0);
    sendOsc("/strip/" + i + "/is_group_expanded", trk.isGroupExpanded().get() ? 1 : 0);
    sendOsc("/strip/" + i + "/activated", stripIsActivated(i) ? 1 : 0);
    sendOsc("/strip/" + i + "/depth", stripDepth[i]);
    sendOsc("/strip/" + i + "/selected", stripSelected[i] ? 1 : 0);
    sendOsc("/strip/" + i + "/exists", 1);
}

// Same depth reconstruction as recomputeDepths, for the strip bank.
function recomputeStripDepths() {
    stripDepthDirty = false;
    var stack = [];
    for (var t = 0; t < STRIP_SIZE; t++) {
        var trk = stripTracks[t];
        var d = 0;
        if (trk && trk.exists().get()) {
            while (stack.length && stack[stack.length - 1] <= 0) stack.pop();
            d = stack.length;
            if (stack.length) stack[stack.length - 1]--;
            if (trk.isGroup().get() && trk.isGroupExpanded().get() && stripChildCount[t] > 0) {
                stack.push(stripChildCount[t]);
            }
        }
        if (d !== stripDepth[t]) {
            stripDepth[t] = d;
            if (trk && trk.exists().get()) sendOsc("/strip/" + t + "/depth", d);
        }
    }
}

function initStripBank() {
    stripBank = host.createMainTrackBank(STRIP_SIZE, 0, 0);
    try { stripBank.followCursorTrack(cursorTrack); } catch(e) {}

    stripBank.scrollPosition().addValueObserver(function(pos) {
        stripOffset = pos;
        stripDepthDirty = true;
        sendOsc("/strip/offset", pos);
    });
    stripBank.canScrollBackwards().addValueObserver(function(c) {
        sendOsc("/strip/can_scroll_back", c ? 1 : 0);
    });
    stripBank.canScrollForwards().addValueObserver(function(c) {
        sendOsc("/strip/can_scroll_forw", c ? 1 : 0);
    });

    for (var t = 0; t < STRIP_SIZE; t++) {
        (function(i) {
            var trk = stripBank.getItemAt(i);
            stripTracks[i] = trk;
            trk.exists().markInterested();
            trk.name().markInterested();
            trk.isGroup().markInterested();
            trk.isGroupExpanded().markInterested();
            try { trk.isActivated().markInterested(); } catch(e) {}

            // Only slots holding a track are sent; a track appearing sends its whole slot.
            trk.exists().addValueObserver(function(e) {
                stripDepthDirty = true;
                if (e) sendStripSlot(i);
                else sendOsc("/strip/" + i + "/exists", 0);
            });
            trk.name().addValueObserver(function(n) {
                if (stripExists(i)) sendOsc("/strip/" + i + "/name", n || "");
            });
            trk.color().addValueObserver(function(r, g, b) {
                stripColors[i] = rgbToHex(r, g, b);
                if (stripExists(i)) sendOsc("/strip/" + i + "/color", stripColors[i]);
            });
            trk.isGroup().addValueObserver(function(g) {
                stripDepthDirty = true;
                if (stripExists(i)) sendOsc("/strip/" + i + "/is_group", g ? 1 : 0);
            });
            trk.isGroupExpanded().addValueObserver(function(x) {
                stripDepthDirty = true;
                if (stripExists(i)) sendOsc("/strip/" + i + "/is_group_expanded", x ? 1 : 0);
            });
            try {
                trk.isActivated().addValueObserver(function(a) {
                    if (stripExists(i)) sendOsc("/strip/" + i + "/activated", a ? 1 : 0);
                });
            } catch(e) {}
            try {
                trk.createTrackBank(1, 0, 0, false).itemCount().addValueObserver(function(n) {
                    // A group's child bank also lists the group's own master track
                    stripChildCount[i] = Math.max(0, n - 1);
                    stripDepthDirty = true;
                });
            } catch(e) {}
            trk.addIsSelectedInEditorObserver(function(sel) {
                stripSelected[i] = sel;
                if (stripExists(i)) sendOsc("/strip/" + i + "/selected", sel ? 1 : 0);
            });
        })(t);
    }
}

var cachedMasterName = "Master";
var cachedMasterColor = "#4a90e2";

var cachedBankMeters = [0, 0, 0, 0, 0, 0, 0, 0];
var lastBankMeters = [0, 0, 0, 0, 0, 0, 0, 0];
var cachedCursorMeter = 0;
var lastCursorMeter = 0;
var cachedCursorMeterL = 0;
var cachedCursorMeterR = 0;
var lastCursorMeterL = 0;
var lastCursorMeterR = 0;

var cachedModulated = [0, 0, 0, 0, 0, 0, 0, 0];
var lastModulated = [-1, -1, -1, -1, -1, -1, -1, -1];

var RUST_APP_PORT = 8081;
var BITWIG_PORT = 8082;

function rgbToHex(r, g, b) {
    var toHex = function(c) {
        var n = Math.max(0, Math.min(255, Math.round(c * 255)));
        var h = n.toString(16);
        return h.length === 1 ? "0" + h : h;
    };
    return "#" + toHex(r) + toHex(g) + toHex(b);
}

function init() {
    debugLog("init START");

    // 1. Setup Bitwig API objects FIRST
    try {
        cursorTrack = host.createCursorTrack("MASCHINE_TRACK", "Maschine Track", 12, 0, true);
        cursorDevice = cursorTrack.createCursorDevice();
        remoteControls = cursorDevice.createCursorRemoteControlsPage(8);
        trackBank = host.createMainTrackBank(32, 6, 0);
        returnTrack = host.createCursorTrack("MASCHINE_RETURN", "Maschine Return Track", 0, 0, false);
        try {
            cursorTrack.setCursorNavigationMode(com.bitwig.extension.controller.api.CursorNavigationMode.FLAT);
        } catch(eNav) {
            try {
                cursorTrack.setCursorNavigationMode(CursorNavigationMode.FLAT);
            } catch(eNav2) {
                debugLog("Could not set CursorNavigationMode FLAT: " + eNav2);
            }
        }
        try {
            trackBank.followCursorTrack(cursorTrack);
            trackBank.cursorIndex().markInterested();
        } catch(e2) {}
        try { initStripBank(); } catch(eStrip) { debugLog("Strip bank error: " + eStrip); }
        masterTrack = host.createMasterTrack(0);
        masterTrack.name().markInterested();
        masterTrack.color().markInterested();
        masterTrack.name().addValueObserver(function(n) {
            cachedMasterName = n || "Master";
            sendOsc("/channel/master/name", cachedMasterName);
        });
        masterTrack.color().addValueObserver(function(r, g, b) {
            var hex = rgbToHex(r, g, b);
            cachedMasterColor = hex;
            sendOsc("/channel/master/color", cachedMasterColor);
        });
        transport = host.createTransport();
        debugLog("Created Bitwig API host cursors and track bank");
    } catch (e) {
        debugLog("Error creating host cursors: " + e);
    }

    // 2. Initialize OSC Module & Start UDP Server and Client
    try {
        var osc = host.getOscModule();
        var addressSpace = osc.createAddressSpace();

        addressSpace.registerDefaultMethod(function(source, msg) {
            var addr = msg.getAddressPattern();
            var args = msg.getArguments();

            try {
                if (addr === "/refresh") {
                    handleRefresh();
                    return;
                }
                if (addr === "/debug/dump") {
                    debugDump();
                    return;
                }

                // 1. Device Parameter Control (Phase 1)
                if (addr.startsWith("/device/param/")) {
                    var parts = addr.split("/");
                    if (parts.length === 5) {
                        var index = parseInt(parts[3], 10);
                        if (index >= 0 && index < 8 && args.length > 0) {
                            var val = args[0];
                            if (parts[4] === "set") {
                                remoteControls.getParameter(index).value().set(val);
                            } else if (parts[4] === "inc") {
                                remoteControls.getParameter(index).inc(val, 128);
                            }
                        }
                    }
                } else if (addr === "/device/nav/next") {
                    cursorDevice.selectNext();
                } else if (addr === "/device/nav/prev") {
                    cursorDevice.selectPrevious();

                // 2. Transport Control
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
                    try {
                        if (typeof transport.preRoll === "function") transport.preRoll().set("1 bar");
                        if (typeof transport.isPreRollEnabled === "function") transport.isPreRollEnabled().set(true);
                    } catch(e) {}
                    transport.isArrangerRecordEnabled().set(true);
                    if (!isCurrentlyPlaying) transport.play();
                } else if (addr === "/transport/restart") {
                    if (isCurrentlyPlaying) {
                        try { transport.jumpToPlaystartPosition(); } catch(e) { transport.restart(); }
                        transport.play();
                    } else {
                        transport.getPosition().set(0);
                        transport.play();
                    }
                } else if (addr === "/transport/loop_toggle") {
                    transport.isArrangerLoopEnabled().toggle();

                // 3. Track Bank & Mixer Control (Phase 2)
                } else if (addr.startsWith("/track/bank/")) {
                    var parts = addr.split("/");
                    if (parts.length >= 4) {
                        if (parts[3] === "scroll_back") {
                            trackBank.scrollBackwards();
                        } else if (parts[3] === "scroll_forw") {
                            trackBank.scrollForwards();
                        } else {
                            var trackIdx = parseInt(parts[3], 10);
                            if (trackIdx >= 0 && trackIdx < 32) {
                                var track = trackBank.getItemAt(trackIdx);
                                var action = parts[4];
                                if (action === "volume" && parts[5] === "inc" && args.length > 0) {
                                    track.volume().inc(args[0], 128);
                                } else if (action === "volume" && parts[5] === "set" && args.length > 0) {
                                    track.volume().value().set(args[0]);
                                } else if (action === "pan" && parts[5] === "inc" && args.length > 0) {
                                    track.pan().inc(args[0], 128);
                                } else if (action === "pan" && parts[5] === "set" && args.length > 0) {
                                    track.pan().value().set(args[0]);
                                } else if (action === "mute" && parts[5] === "toggle") {
                                    track.mute().toggle();
                                } else if (action === "solo" && parts[5] === "toggle") {
                                    track.solo().toggle();
                                } else if (action === "arm" && parts[5] === "toggle") {
                                    track.arm().toggle();
                                } else if (action === "select") {
                                    cursorTrack.selectChannel(track);
                                    try { track.selectInEditor(); } catch(e1) {}
                                    try { track.selectInMixer(); } catch(e2) {}
                                } else if (action === "expand" && args.length > 0) {
                                    if (track.isGroup().get()) track.isGroupExpanded().set(Number(args[0]) === 1);
                                }
                            }
                        }
                    }

                // Channel strip bank (4D-encoder navigation)
                } else if (addr.startsWith("/strip/")) {
                    var sp = addr.split("/");
                    var si = parseInt(sp[2], 10);
                    if (si >= 0 && si < STRIP_SIZE && stripExists(si)) {
                        var st = stripTracks[si];
                        if (sp[3] === "select") {
                            cursorTrack.selectChannel(st);
                            try { st.selectInEditor(); } catch(e1) {}
                            try { st.selectInMixer(); } catch(e2) {}
                        } else if (sp[3] === "expand" && args.length > 0) {
                            if (st.isGroup().get()) st.isGroupExpanded().set(Number(args[0]) === 1);
                        }
                    }

                // 4. Track Navigation
                } else if (addr === "/track/nav/prev") {
                    // Fallback used when the target is outside the bank window
                    cursorTrack.selectPrevious();
                } else if (addr === "/track/nav/next") {
                    cursorTrack.selectNext();

                // 5. Single Focused Track / Channel Mode
                } else if (addr.startsWith("/channel/")) {
                    var parts = addr.split("/");
                    if (parts[2] === "volume" && parts[3] === "inc" && args.length > 0) {
                        cursorTrack.volume().inc(args[0], 128);
                    } else if (parts[2] === "pan" && parts[3] === "inc" && args.length > 0) {
                        cursorTrack.pan().inc(args[0], 128);
                    } else if (parts[2] === "mute" && parts[3] === "toggle") {
                        cursorTrack.mute().toggle();
                    } else if (parts[2] === "solo" && parts[3] === "toggle") {
                        cursorTrack.solo().toggle();
                    } else if (parts[2] === "arm" && parts[3] === "toggle") {
                        cursorTrack.arm().toggle();
                    } else if (parts[2] === "master" && parts[3] === "toggle") {
                        try {
                            if (isViewingMaster) {
                                cursorTrack.selectChannel(returnTrack);
                                isViewingMaster = false;
                            } else {
                                // Remember the current track in its own cursor, so it survives bank scrolling
                                returnTrack.selectChannel(cursorTrack);
                                cursorTrack.selectChannel(masterTrack);
                                isViewingMaster = true;
                            }
                            sendOsc("/channel/is_master", isViewingMaster ? 1 : 0);
                        } catch(e) {
                            debugLog("Master toggle error: " + e);
                        }
                    } else if (parts[2] === "monitor" && parts[3] === "cycle") {
                        try {
                            if (typeof cursorTrack.monitorMode === "function") {
                                cursorTrack.monitorMode().setNext();
                            } else if (typeof cursorTrack.isMonitoring === "function") {
                                cursorTrack.isMonitoring().toggle();
                            }
                        } catch(e) {}
                    } else if (parts[2] === "nav" && parts[3] === "parent") {
                        // Parent group is outside the bank window: move up via the cursor, then close it
                        cursorTrack.selectParent();
                        host.scheduleTask(function() {
                            if (cursorTrack.isGroup().get()) cursorTrack.isGroupExpanded().set(false);
                        }, 80);
                    } else if (parts[2] === "group" && parts[3] === "toggle") {
                        try {
                            var isGrp = cursorTrack.isGroup().get();
                            debugLog("OSC /channel/group/toggle: isGroup=" + isGrp + ", curTrack=" + cursorTrack.name().get());
                            if (isGrp) {
                                cursorTrack.isGroupExpanded().toggle();
                                debugLog("Toggled group expanded to: " + cursorTrack.isGroupExpanded().get());
                            }
                        } catch(e) {
                            debugLog("Group toggle error: " + e);
                        }
                    } else if (parts[2] === "send" && parts.length === 5 && parts[4] === "inc" && args.length > 0) {
                        var sendIdx = parseInt(parts[3], 10);
                        if (sendIdx >= 0 && sendIdx < 12) {
                            cursorTrack.sendBank().getItemAt(sendIdx).inc(args[0], 128);
                        }
                    }
                }
            } catch(e) {
                debugLog("OSC Method Handler Error: " + e);
            }
        });

        oscServer = osc.createUdpServer(BITWIG_PORT, addressSpace);
        oscClient = osc.connectToUdpServer("127.0.0.1", RUST_APP_PORT, osc.createAddressSpace());
        debugLog("OSC UDP Server started on port " + BITWIG_PORT + ", Client connected to " + RUST_APP_PORT);
    } catch(e) {
        debugLog("Error initializing OSC: " + e);
    }

    // Observers: Transport
    try {
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
        debugLog("Transport observers registered");
    } catch(e) { debugLog("Transport observer error: " + e); }

    // Observers: Focused Track
    try {
        cursorTrack.name().markInterested();
        cursorTrack.name().addValueObserver(function(name) {
            var n = name || "No Track";
            sendOsc("/channel/name", n);
            var isMaster = (n.toLowerCase() === "master") ? 1 : 0;
            isViewingMaster = (isMaster === 1);
            sendOsc("/channel/is_master", isMaster);
        });

        cursorTrack.color().markInterested();
        cursorTrack.color().addValueObserver(function(r, g, b) {
            cachedCursorColor = rgbToHex(r, g, b);
            sendOsc("/channel/color", cachedCursorColor);
        });

        cursorTrack.volume().markInterested();
        cursorTrack.volume().value().markInterested();
        cursorTrack.volume().displayedValue().markInterested();
        cursorTrack.volume().value().addValueObserver(function(val) {
            sendOsc("/channel/volume", val);
        });
        cursorTrack.volume().displayedValue().addValueObserver(function(valStr) {
            sendOsc("/channel/volume_display", valStr || "");
        });

        cursorTrack.pan().markInterested();
        cursorTrack.pan().value().markInterested();
        cursorTrack.pan().displayedValue().markInterested();
        cursorTrack.pan().value().addValueObserver(function(val) {
            sendOsc("/channel/pan", val);
        });
        cursorTrack.pan().displayedValue().addValueObserver(function(valStr) {
            sendOsc("/channel/pan_display", valStr || "C");
        });

        cursorTrack.mute().markInterested();
        cursorTrack.mute().addValueObserver(function(isMuted) {
            cachedTrackMuted = isMuted ? 1 : 0;
            sendOsc("/track/mute", cachedTrackMuted);
            sendOsc("/channel/mute", cachedTrackMuted);
        });

        cursorTrack.solo().markInterested();
        cursorTrack.solo().addValueObserver(function(isSolo) {
            sendOsc("/channel/solo", isSolo ? 1 : 0);
        });

        cursorTrack.arm().markInterested();
        cursorTrack.arm().addValueObserver(function(isArm) {
            sendOsc("/channel/arm", isArm ? 1 : 0);
        });

        try {
            if (typeof cursorTrack.monitorMode === "function") {
                cursorTrack.monitorMode().markInterested();
                cursorTrack.monitorMode().addValueObserver(function(m) {
                    cachedMonitorMode = m ? m.toUpperCase() : "AUTO";
                    sendOsc("/channel/monitor/mode", cachedMonitorMode);
                });
            }
        } catch(e) {}

        try {
            cursorTrack.isGroup().markInterested();
            cursorTrack.isGroup().addValueObserver(function(g) {
                cachedIsGroup = g ? 1 : 0;
                sendOsc("/channel/is_group", cachedIsGroup);
            });
            cursorTrack.isGroupExpanded().markInterested();
            cursorTrack.isGroupExpanded().addValueObserver(function(ge) {
                cachedCursorGroupExpanded = ge ? 1 : 0;
                sendOsc("/channel/is_group_expanded", cachedCursorGroupExpanded);
            });
        } catch(e) {}

        cursorTrack.addVuMeterObserver(100, -1, true, function(level) {
            cachedCursorMeter = level / 100.0;
        });
        cursorTrack.addVuMeterObserver(100, 0, true, function(level) {
            cachedCursorMeterL = level / 100.0;
        });
        cursorTrack.addVuMeterObserver(100, 1, true, function(level) {
            cachedCursorMeterR = level / 100.0;
        });
    } catch(e) { println("CursorTrack observer error: " + e); }

    // Observers: Focused Track Sends (0 to 11)
    try {
        var sendBank = cursorTrack.sendBank();
        for (var s = 0; s < 12; s++) {
            (function(index) {
                try {
                    var send = sendBank.getItemAt(index);
                    cursorSends[index] = send;
                    if (send) {
                        send.name().markInterested();
                        send.value().markInterested();
                        send.displayedValue().markInterested();
                        send.exists().markInterested();
                        send.name().addValueObserver(function(n) {
                            sendOsc("/channel/send/" + index + "/name", n || ("Send " + String.fromCharCode(65 + index)));
                        });
                        send.value().addValueObserver(function(v) {
                            sendOsc("/channel/send/" + index + "/value", typeof v === "number" ? v : 0.0);
                        });
                        send.displayedValue().addValueObserver(function(d) {
                            sendOsc("/channel/send/" + index + "/display", d || "");
                        });
                        send.exists().addValueObserver(function(e) {
                            var total = 0;
                            for (var j = 0; j < 12; j++) {
                                if (cursorSends[j] && cursorSends[j].exists().get()) {
                                    total = j + 1;
                                }
                            }
                            sendOsc("/channel/sends/total", total);
                        });
                    }
                } catch(e1) { println("Send " + index + " observer error: " + e1); }
            })(s);
        }
    } catch(e) { println("SendBank observer error: " + e); }

    // Observers: Device Parameters (Phase 1)
    try {
        cursorDevice.name().markInterested();
        cursorDevice.name().addValueObserver(function(name) {
            cachedDeviceName = name || "No Device";
            sendOsc("/device/name", cachedDeviceName);
        });

        for (var i = 0; i < 8; i++) {
            var param = remoteControls.getParameter(i);
            params[i] = param;
            if (param) {
                param.name().markInterested();
                param.value().markInterested();
                param.displayedValue().markInterested();
                param.modulatedValue().markInterested();

                (function(index) {
                    param.name().addValueObserver(function(name) {
                        sendOsc("/device/param/" + index + "/name", name || "");
                    });
                    param.value().addValueObserver(function(value) {
                        sendOsc("/device/param/" + index + "/value", typeof value === "number" ? value : 0.0);
                    });
                    param.displayedValue().addValueObserver(function(displayStr) {
                        sendOsc("/device/param/" + index + "/display", displayStr || "");
                    });
                })(i);
            }
        }
    } catch(e) { println("Device observer error: " + e); }

    // Observers: Track Bank (Phase 2 - 8 Tracks)
    try {
        trackBank.scrollPosition().markInterested();
        trackBank.scrollPosition().addValueObserver(function(pos) {
            depthDirty = true;
            cachedBankOffset = pos;
            sendOsc("/track/bank/offset", pos);
        });

        trackBank.canScrollBackwards().markInterested();
        trackBank.canScrollBackwards().addValueObserver(function(canScroll) {
            cachedBankCanScrollBack = canScroll ? 1 : 0;
            sendOsc("/track/bank/can_scroll_back", cachedBankCanScrollBack);
        });

        trackBank.canScrollForwards().markInterested();
        trackBank.canScrollForwards().addValueObserver(function(canScroll) {
            cachedBankCanScrollForw = canScroll ? 1 : 0;
            sendOsc("/track/bank/can_scroll_forw", cachedBankCanScrollForw);
        });

        for (var t = 0; t < 32; t++) {
            (function(index) {
                try {
                    var track = trackBank.getItemAt(index);
                    bankTracks[index] = track;

                    if (track) {
                        track.name().markInterested();
                        track.color().markInterested();
                        track.volume().markInterested();
                        track.volume().value().markInterested();
                        track.volume().displayedValue().markInterested();
                        track.pan().markInterested();
                        track.pan().value().markInterested();
                        track.pan().displayedValue().markInterested();
                        track.mute().markInterested();
                        track.solo().markInterested();
                        track.arm().markInterested();
                        track.exists().markInterested();
                        track.isGroup().markInterested();
                        track.isGroupExpanded().markInterested();
                        try { track.isActivated().markInterested(); } catch(e) {}
                
                        track.name().addValueObserver(function(n) {
                            sendOsc("/track/bank/" + index + "/name", n || ("Track " + (index + 1)));
                        });
                        track.color().addValueObserver(function(r, g, b) {
                            var hex = rgbToHex(r, g, b);
                            cachedBankColors[index] = hex;
                            sendOsc("/track/bank/" + index + "/color", hex);
                        });
                        track.volume().value().addValueObserver(function(v) {
                            sendOsc("/track/bank/" + index + "/volume", typeof v === "number" ? v : 0.0);
                        });
                        track.volume().displayedValue().addValueObserver(function(d) {
                            sendOsc("/track/bank/" + index + "/volume_display", d || "");
                        });
                        track.pan().value().addValueObserver(function(p) {
                            sendOsc("/track/bank/" + index + "/pan", typeof p === "number" ? p : 0.5);
                        });
                        track.pan().displayedValue().addValueObserver(function(d) {
                            sendOsc("/track/bank/" + index + "/pan_display", d || "C");
                        });
                        track.mute().addValueObserver(function(m) {
                            sendOsc("/track/bank/" + index + "/mute", m ? 1 : 0);
                        });
                        track.solo().addValueObserver(function(s) {
                            sendOsc("/track/bank/" + index + "/solo", s ? 1 : 0);
                        });
                        track.arm().addValueObserver(function(a) {
                            sendOsc("/track/bank/" + index + "/arm", a ? 1 : 0);
                        });
                        track.exists().addValueObserver(function(e) {
                            depthDirty = true;
                            sendOsc("/track/bank/" + index + "/exists", e ? 1 : 0);
                        });
                        track.isGroup().addValueObserver(function(isGrp) {
                            depthDirty = true;
                            sendOsc("/track/bank/" + index + "/is_group", isGrp ? 1 : 0);
                        });
                        try {
                            track.isActivated().addValueObserver(function(act) {
                                cachedActivated[index] = act;
                                sendOsc("/track/bank/" + index + "/activated", act ? 1 : 0);
                            });
                        } catch(ea) {}
                        try {
                            var childBank = track.createTrackBank(1, 0, 0, false);
                            childBank.itemCount().addValueObserver(function(n) {
                                // A group's child bank also lists the group's own master track
                                cachedChildCount[index] = Math.max(0, n - 1);
                                depthDirty = true;
                            });
                        } catch(ec) {
                            debugLog("Child bank error for track " + index + ": " + ec);
                        }
                        track.isGroupExpanded().addValueObserver(function(exp) {
                            depthDirty = true;
                            cachedIsGroupExpanded[index] = exp ? 1 : 0;
                            sendOsc("/track/bank/" + index + "/is_group_expanded", exp ? 1 : 0);
                        });

                        track.addIsSelectedInEditorObserver(function(isSelected) {
                            bankSelected[index] = isSelected;
                            sendOsc("/track/bank/" + index + "/selected", isSelected ? 1 : 0);
                        });

                        try {
                            var parentTrack = track.createParentTrack(0, 0);
                            bankParentTracks[index] = parentTrack;
                            if (parentTrack) {
                                parentTrack.exists().markInterested();
                                parentTrack.color().markInterested();
                                parentTrack.name().markInterested();

                                parentTrack.exists().addValueObserver(function(exists) {
                                    cachedParentExists[index] = exists ? 1 : 0;
                                    sendOsc("/track/bank/" + index + "/has_parent", exists ? 1 : 0);
                                });
                                parentTrack.color().addValueObserver(function(r, g, b) {
                                    var hex = rgbToHex(r, g, b);
                                    cachedParentColors[index] = hex;
                                    sendOsc("/track/bank/" + index + "/parent_color", hex);
                                });
                                parentTrack.name().addValueObserver(function(n) {
                                    cachedParentNames[index] = n || "";
                                    sendOsc("/track/bank/" + index + "/parent_name", n || "");
                                });

                                try {
                                    var grandparentTrack = parentTrack.createParentTrack(0, 0);
                                    if (grandparentTrack) {
                                        grandparentTrack.exists().markInterested();
                                        grandparentTrack.color().markInterested();
                                        grandparentTrack.name().markInterested();
                                        grandparentTrack.exists().addValueObserver(function(exists) {
                                            sendOsc("/track/bank/" + index + "/has_grandparent", exists ? 1 : 0);
                                        });
                                        grandparentTrack.color().addValueObserver(function(r, g, b) {
                                            var hex = rgbToHex(r, g, b);
                                            cachedGrandparentColors[index] = hex;
                                            sendOsc("/track/bank/" + index + "/grandparent_color", hex);
                                        });
                                        grandparentTrack.name().addValueObserver(function(n) {
                                            sendOsc("/track/bank/" + index + "/grandparent_name", n || "");
                                        });
                                    }
                                } catch(egp) {}
                            }
                        } catch(ep) {
                            println("Parent track observer error for track " + index + ": " + ep);
                        }

                        track.addVuMeterObserver(100, -1, true, function(level) {
                            cachedBankMeters[index] = level / 100.0;
                        });
                    }
                } catch(e1) { println("Bank track " + index + " observer error: " + e1); }
            })(t);
        }
    } catch(e) { println("TrackBank observer error: " + e); }

    pollTick();
    debugLog("Maschine Mk3 (OSC Bridge v3.0) initialized on port " + BITWIG_PORT);
    try {
        host.showPopupNotification("Maschine Mk3 OSC v3.0 Ready");
    } catch(e) {}
}

// Writes the bank as the script sees it to the log file (trigger with OSC /debug/dump on port 8082).
function debugDump() {
    var sends = [];
    for (var si = 0; si < 12; si++) {
        var snd = cursorSends[si];
        if (snd && snd.exists().get()) sends.push(si + ":" + snd.name().get());
    }
    debugLog("---- sends: " + sends.join(" | "));
    debugLog("---- dump: offset=" + cachedBankOffset + " cursor=" + cursorTrack.name().get() + " master=" + isViewingMaster);
    for (var t = 0; t < 32; t++) {
        var trk = bankTracks[t];
        if (!trk || !trk.exists().get()) continue;
        var act = "?";
        try { act = trk.isActivated().get(); } catch(e) {}
        debugLog("  [" + t + "] depth=" + cachedDepth[t] + " children=" + cachedChildCount[t] + " grp=" + trk.isGroup().get() +
            " exp=" + cachedIsGroupExpanded[t] + " sel=" + bankSelected[t] + " act=" + act +
            " name=" + trk.name().get());
    }
    var n = 0;
    for (var s = 0; s < STRIP_SIZE; s++) if (stripExists(s)) n++;
    debugLog("---- strip: offset=" + stripOffset + " tracks=" + n);
    for (var s2 = 0; s2 < STRIP_SIZE; s2++) {
        if (!stripExists(s2)) continue;
        var st = stripTracks[s2];
        debugLog("  {" + s2 + "} depth=" + stripDepth[s2] + " children=" + stripChildCount[s2] + " grp=" + st.isGroup().get() +
            " exp=" + st.isGroupExpanded().get() + " sel=" + stripSelected[s2] + " act=" + stripIsActivated(s2) +
            " name=" + st.name().get());
    }
}

function handleRefresh() {
    try {
        sendOsc("/transport/play", transport.isPlaying().get() ? 1 : 0);
        sendOsc("/transport/rec", transport.isArrangerRecordEnabled().get() ? 1 : 0);
        sendOsc("/track/mute", cursorTrack.mute().get() ? 1 : 0);
        sendOsc("/device/name", cursorDevice.name().get() || "No Device");
    } catch(e) { println("handleRefresh transport error: " + e); }

    // Refresh Device Params
    for (var i = 0; i < 8; i++) {
        try {
            var param = params[i];
            if (param) {
                sendOsc("/device/param/" + i + "/name", param.name().get() || "");
                sendOsc("/device/param/" + i + "/display", param.displayedValue().get() || "");
                sendOsc("/device/param/" + i + "/value", param.value().get());
                sendOsc("/device/param/" + i + "/modulated", param.modulatedValue().get());
            }
        } catch(e) { println("handleRefresh param error: " + e); }
    }

    // Refresh Track Bank
    try {
        sendOsc("/track/bank/offset", cachedBankOffset);
        sendOsc("/track/bank/can_scroll_back", cachedBankCanScrollBack);
        sendOsc("/track/bank/can_scroll_forw", cachedBankCanScrollForw);
    } catch(e) {}

    for (var t = 0; t < 32; t++) {
        try {
            var trk = bankTracks[t];
            if (trk && trk.exists().get()) {
                if (t === 0) {
                    debugLog("Track methods: select=" + typeof trk.select + ", selectInEditor=" + typeof trk.selectInEditor + ", selectInMixer=" + typeof trk.selectInMixer + ", selectChannel=" + typeof trk.selectChannel);
                }
                debugLog("Bank[" + t + "] name=" + trk.name().get() + " sel=" + (bankSelected[t] ? "YES" : "no") + " grp=" + trk.isGroup().get() + " exp=" + cachedIsGroupExpanded[t]);
                sendOsc("/track/bank/" + t + "/name", trk.name().get() || ("Track " + (t + 1)));
                sendOsc("/track/bank/" + t + "/color", cachedBankColors[t] || "#ff8c1a");
                sendOsc("/track/bank/" + t + "/volume", trk.volume().value().get());
                sendOsc("/track/bank/" + t + "/volume_display", trk.volume().displayedValue().get() || "");
                sendOsc("/track/bank/" + t + "/pan", trk.pan().value().get());
                sendOsc("/track/bank/" + t + "/pan_display", trk.pan().displayedValue().get() || "C");
                sendOsc("/track/bank/" + t + "/mute", trk.mute().get() ? 1 : 0);
                sendOsc("/track/bank/" + t + "/solo", trk.solo().get() ? 1 : 0);
                sendOsc("/track/bank/" + t + "/arm", trk.arm().get() ? 1 : 0);
                sendOsc("/track/bank/" + t + "/exists", trk.exists().get() ? 1 : 0);
                sendOsc("/track/bank/" + t + "/is_group", trk.isGroup().get() ? 1 : 0);
                sendOsc("/track/bank/" + t + "/is_group_expanded", cachedIsGroupExpanded[t] || 0);
                sendOsc("/track/bank/" + t + "/depth", cachedDepth[t]);
                sendOsc("/track/bank/" + t + "/activated", cachedActivated[t] ? 1 : 0);
                sendOsc("/track/bank/" + t + "/selected", bankSelected[t] ? 1 : 0);
                sendOsc("/track/bank/" + t + "/has_parent", cachedParentExists[t] ? 1 : 0);
                sendOsc("/track/bank/" + t + "/parent_color", cachedParentColors[t] || "");
                sendOsc("/track/bank/" + t + "/parent_name", cachedParentNames[t] || "");
                sendOsc("/track/bank/" + t + "/has_grandparent", cachedGrandparentColors[t] ? 1 : 0);
                sendOsc("/track/bank/" + t + "/grandparent_color", cachedGrandparentColors[t] || "");
            }
        } catch(e) { println("handleRefresh bank error " + t + ": " + e); }
    }

    // Refresh Channel Strip bank
    try {
        sendOsc("/strip/offset", stripOffset);
        sendOsc("/strip/can_scroll_back", stripBank.canScrollBackwards().get() ? 1 : 0);
        sendOsc("/strip/can_scroll_forw", stripBank.canScrollForwards().get() ? 1 : 0);
        for (var sr = 0; sr < STRIP_SIZE; sr++) {
            if (stripExists(sr)) sendStripSlot(sr);
        }
    } catch(e) { println("handleRefresh strip error: " + e); }

    // Refresh Focused Channel
    try {
        sendOsc("/channel/name", cursorTrack.name().get() || "No Track");
        sendOsc("/channel/color", cachedCursorColor || "#ff8c1a");
        sendOsc("/channel/volume", cursorTrack.volume().value().get());
        sendOsc("/channel/volume_display", cursorTrack.volume().displayedValue().get() || "");
        sendOsc("/channel/pan", cursorTrack.pan().value().get());
        sendOsc("/channel/pan_display", cursorTrack.pan().displayedValue().get() || "C");
        sendOsc("/channel/mute", cursorTrack.mute().get() ? 1 : 0);
        sendOsc("/channel/solo", cursorTrack.solo().get() ? 1 : 0);
        sendOsc("/channel/arm", cursorTrack.arm().get() ? 1 : 0);
        sendOsc("/channel/is_master", isViewingMaster ? 1 : 0);
        sendOsc("/channel/is_group", cachedIsGroup);
        sendOsc("/channel/is_group_expanded", cachedCursorGroupExpanded);
        sendOsc("/channel/master/name", cachedMasterName);
        sendOsc("/channel/master/color", cachedMasterColor);
        sendOsc("/channel/monitor/mode", cachedMonitorMode);
    } catch(e) { println("handleRefresh channel error: " + e); }

    var totalSends = 0;
    for (var s = 0; s < 12; s++) {
        try {
            var snd = cursorSends[s];
            if (snd) {
                if (snd.exists().get()) totalSends = s + 1;
                sendOsc("/channel/send/" + s + "/name", snd.name().get() || ("Send " + String.fromCharCode(65 + s)));
                sendOsc("/channel/send/" + s + "/value", snd.value().get());
                sendOsc("/channel/send/" + s + "/display", snd.displayedValue().get() || "");
            }
        } catch(e) {}
    }
    sendOsc("/channel/sends/total", totalSends);
}

function pollTick() {
    if (depthDirty) {
        try { recomputeDepths(); } catch(e) { debugLog("recomputeDepths error: " + e); }
    }
    if (stripDepthDirty) {
        try { recomputeStripDepths(); } catch(e) { debugLog("recomputeStripDepths error: " + e); }
    }

    // 1. Device Modulation Poll
    for (var i = 0; i < 8; i++) {
        try {
            if (params[i]) {
                var modValue = params[i].modulatedValue().get();
                if (typeof modValue === "number") {
                    cachedModulated[i] = modValue;
                    if (Math.abs(modValue - lastModulated[i]) > 0.002) {
                        lastModulated[i] = modValue;
                        sendOsc("/device/param/" + i + "/modulated", modValue);
                    }
                }
            }
        } catch(e) {}
    }

    // 2. VU Peak Meter Poll (Throttled real-time transmission)
    for (var t = 0; t < 8; t++) {
        try {
            var m = cachedBankMeters[t];
            if (m > 0.01 || lastBankMeters[t] > 0.01) {
                if (Math.abs(m - lastBankMeters[t]) > 0.01) {
                    lastBankMeters[t] = m;
                    sendOsc("/track/bank/" + t + "/meter", m);
                }
            }
        } catch(e) {}
    }

    try {
        if (cachedCursorMeterL > 0.01 || lastCursorMeterL > 0.01 ||
            cachedCursorMeterR > 0.01 || lastCursorMeterR > 0.01 ||
            cachedCursorMeter > 0.01 || lastCursorMeter > 0.01) {
            if (Math.abs(cachedCursorMeterL - lastCursorMeterL) > 0.01 ||
                Math.abs(cachedCursorMeterR - lastCursorMeterR) > 0.01 ||
                Math.abs(cachedCursorMeter - lastCursorMeter) > 0.01) {
                lastCursorMeterL = cachedCursorMeterL;
                lastCursorMeterR = cachedCursorMeterR;
                lastCursorMeter = cachedCursorMeter;
                sendOsc("/channel/meter", cachedCursorMeterL, cachedCursorMeterR);
            }
        }
    } catch(e) {}

    host.scheduleTask(pollTick, TICK_RATE_MS);
}

function sendOsc(address) {
    if (oscClient) {
        try {
            var args = Array.prototype.slice.call(arguments, 1);
            if (args.length === 0) {
                oscClient.sendMessage(address);
            } else if (args.length === 1) {
                oscClient.sendMessage(address, args[0]);
            } else if (args.length === 2) {
                oscClient.sendMessage(address, args[0], args[1]);
            } else if (args.length === 3) {
                oscClient.sendMessage(address, args[0], args[1], args[2]);
            }
        } catch (e) {
            println("OSC Send Error: " + e);
        }
    }
}
