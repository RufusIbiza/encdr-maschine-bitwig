# Notes from a parallel build

These are notes offered from the outside, from someone who built a comparable
thing on different hardware against a different host: a **Maschine MK2** driving
**Zynthian** (a Raspberry Pi synth/sequencer platform), with a Rust HID daemon
talking to a Python control-surface driver.

That project lives here:
**<https://github.com/Witzman/Generative-Techno-ZynthianMaschine-MKII>** — a
generative techno instrument played entirely from the MK2 (euclidean drum
channels, Turing-machine melodic voices, latched mode pages, live pad recording).
Everything referenced below as "mine" or "my project" is in that repository, so
any claim here can be checked against the code rather than taken on trust.

The architectures rhyme. Both projects concluded that the host's own extension
sandbox is the wrong place to own USB HID, and both ended up with a native
process on one side, a thin bridge in the host on the other, and a message
protocol between them. Because of that, several problems that took me a long
time to diagnose are ones this project either has already or will meet on the
way through the roadmap.

Nothing here is a demand, and none of it is a bug report I have reproduced —
I read this repository, I have not run it. Where I point at a line, treat it as
"this is the shape I mean", not "this is broken". Where I say something cost me
real time, that part is measured.

Both projects are GPL-3.0, so anything below can be lifted freely.

---

## The list

| # | Item | Relevance |
|---|---|---|
| 1 | **Encoder pages as data, with a "shape" field** | Phase 2 and Phase 4 — the single biggest one |
| 2 | **Mirrored host state must be re-read, never assumed** | Present now; latent defect on start order |
| 3 | **LEDs need explicit re-derive plus a cache clear** | Same defect, output side |
| 4 | **Never do slow work on the input thread** | Present now, via the modulation poll |
| 5 | **Push logic out of the I/O loop so it can be unit tested** | Structural, gets harder to do later |
| 6 | **Silence must explain itself** | Cost me a jam; cheap rule |
| 7 | **Latched vs momentary modifier behaviour** | Phase 3 |
| 8 | **Recording played notes over a pattern the machine also writes** | Phase 3, if a generator ever appears |

---

## 1. Encoder pages as data, with a "shape" field

**Where this bites:** `src/main.rs:156-185`. Right now the eight encoders map to
the eight remote-control parameters through a `position()` lookup and a fixed
OSC address. That is exactly right for Phase 1.

Phase 2 wants volume, pan and sends across *tracks*. That is a different
relation: not eight verbs applied to one selected thing, but **one verb applied
to eight different things**. Phase 4 (browser) is a third relation again. The
path of least resistance is a second `match` arm, then a third, and then the
encoder handler is where all the knowledge lives and nothing is addressable by
name.

I shipped the first version that way and had to undo it.

**What replaced it.** A page is a descriptor, held in a table, and it carries a
**shape**. The shape is the whole trick. Three shapes covered everything I have
needed so far:

- **`channel`** — eight different verbs, all applied to the one selected target.
  This is your current layout: eight parameters of the selected device.
- **`spread`** — one verb, applied across eight targets. This is the whole of
  your Phase 2: `volume` across tracks 1-8, then `pan` across tracks 1-8, then
  send A across tracks 1-8. Also level meters, mute, solo.
- **`global`** — eight unrelated globals (tempo, swing, master level, …).

The dispatcher then becomes a three-way branch that computes `(verb, target)`
from `(shape, page, encoder_index)`, and calls one unchanged function:

```
verb_apply(verb, target, delta)
```

Adding a mixer page is a table entry. Adding a sends page is a table entry.
Nothing in the encoder handler learns anything new.

Two things I got wrong doing this, worth having in advance:

- The table is keyed by **`(mode, kind)`**, not by mode alone. "Kind" is what the
  selected target *is* — in my case a drum channel versus a melodic voice, in
  yours plausibly an instrument track versus an effect track versus the master.
  The same physical page means different things depending on kind, and
  retrofitting that key later meant touching every entry.
- My `verb_apply` asked the target for its kind on its **first line**, which
  meant a `global` page could not pass a null target — it raised before reaching
  the branch that did not need a target at all. Globals pass the selected
  target and ignore it. Trivial, but it cost an afternoon.

**Bitwig-specific bonus.** `createCursorRemoteControlsPage(8)`
(`bitwig-extension/encdr-maschine-mk3.control.js:40`) already exposes
`selectedPageIndex()` and a page count. That is a page ring the host hands you
for free — it fits straight into the same descriptor model rather than sitting
beside it as a special case, and it means the eight top buttons or the 4-D
encoder can page through device parameter banks with no table in the Rust app
knowing any plugin's parameter names.

I generate my equivalent pages entirely from whatever the audio chain publishes,
so no table in my code knows any specific plugin's ports. That has been one of
the highest-value decisions in the project. One warning from it: filter out the
host's own synthetic ports. Mine were surfacing `lv2_freewheel`, `latency` and
`enabled` as if they were musical parameters.

---

## 2. Mirrored host state must be re-read, never assumed

This is the one I would most want to receive if the direction were reversed,
because the failure mode is *silent* and it makes the surface confidently wrong.

**The shape in this repo.** `src/main.rs:70-74` holds `device_name`,
`param_names`, `param_displays`, `param_values` and `param_modulated`. Those are
populated exclusively by inbound OSC. On the Bitwig side,
`encdr-maschine-mk3.control.js:26-28` keeps `lastValues` / `lastNames` and
suppresses any send where the value has not changed.

Now consider start order. Bitwig loads the controller script; `init()` runs;
Bitwig fires every value observer once on registration; those OSC packets go out
over UDP to port 8081 where **nothing is listening yet**, so they are silently
dropped. But `lastValues` and `lastNames` are now populated. When the Rust app
starts a moment later, the script has nothing new to say and will not repeat
itself. The screens stay empty, and there is no path back except changing a
value or selecting a different device — `lastNames`/`lastValues` are cleared
only inside the `cursorDevice.name()` observer at
`encdr-maschine-mk3.control.js:134-136`.

The Rust app restarting mid-session has the same result.

**Why I care so much about this one.** My equivalent bug was per-pattern
properties — play-chance and swing — which the driver mirrored but defaulted to
100 on load instead of reading back. A pattern saved at chance 0 came back
silent, while the surface read 100 and drew the channel as healthy. The one
mechanism the instrument had for explaining silence was actively reporting that
nothing was wrong. It took a long time to find precisely because every display
agreed with every other display, and all of them were downstream of the same bad
assumption.

**The fix is small.** Add a `/refresh` (or `/hello`) method to the script's
address space that clears `lastValues`, `lastNames` and `lastModulatedValues`
and re-emits everything, and have the Rust app send it once on startup —
ideally re-send it periodically until the first inbound packet arrives, so the
opposite start order also heals.

**The general rule, stated the way I now hold it:** *any host state the surface
mirrors must have a path that re-reads it from the host. Never a path that
assumes it.* If there is no way to read a piece of state back, that is worth
knowing explicitly, because it means that state cannot survive a reconnect and
the UI should not pretend otherwise.

---

## 3. LEDs need an explicit re-derive, and the LED cache must be cleared

Same defect as #2, on the output side, and it is worth stating separately because
the fix for #2 does not automatically fix it.

`src/main.rs:27-36` initialises every button LED to 0 and sets `stop` to 255,
on the assumption that the transport starts stopped. After that, LED state is
purely a function of inbound OSC. If Bitwig is playing when the Rust app starts,
the surface shows stopped until the next transport change.

The part that caught me out: I added the re-derive, and **the LEDs still did not
repaint**. My LED layer keeps a cache of last-written values so it can skip
unchanged writes, and the re-derived values were "unchanged" relative to that
stale cache, so every write was suppressed. The re-derive has to **clear the LED
cache too**, or it does nothing at all and looks like the re-derive itself is
broken.

---

## 4. Never do slow work on the thread that reads input

**Where this bites:** the main loop in `src/main.rs` drains USB events
(`156`), pumps the WebView, and calls `capture_and_submit` for both screens
(`265-271`) — all on one thread, cycling every 8 ms (`281`).

A WebKit snapshot is not a bounded-cost operation. While it runs, USB input is
not being drained.

This is not hypothetical here, because of the modulation poll. The Bitwig script
polls `modulatedValue()` every 40 ms
(`encdr-maschine-mk3.control.js:181-190`) and sends on any change. With a
single LFO mapped to a macro, that is a change essentially every poll, which sets
`dirty = true`, which triggers a **full WebKit capture and USB submit of both
screens up to 25 times a second**, on the thread that also reads the hardware.
An LFO on a macro is the exact case the modulation tick was built to show, so
the worst case is also the demo case.

I suspect this is what the periodic forced resubmit at `src/main.rs:274-279`
is compensating for. The comment says it is there so the screens do not
"permanently drop out if a capture event is missed" — a missed capture usually
means something upstream stalled, and a keyframe timer papers over it rather
than removing it.

**My version of this lesson was more expensive.** My MIDI event handler held a
lock for the duration of each event, and I made it load a synth preset inline.
An engine load blocks on a socket for *seconds*. It froze the entire instrument
and needed a full restart. The fix was to never do it on that thread: the event
handler now records an intent, and a separate poll thread performs the load.

Concretely for this project: move rendering and capture to their own thread, and
let the input drain run uninterrupted. A second, related rate limit that paid off
for me — coalesce state changes and render on a fixed timer rather than on every
change. Redrawing per inbound event starved my input reader badly enough to trip
a hardware watchdog; a fixed ~100 ms render timer removed it entirely.

---

## 5. Push logic out of the I/O loop so it can be unit tested

`src/main.rs` is a single 283-line loop with all behaviour inline. That is a
completely reasonable shape for a proof of concept, and I am not suggesting
architecture for its own sake. The reason to move now rather than later is that
the roadmap is where the testable logic actually appears: scale mapping and
chromatic layouts in Phase 3, step-sequencer state, browser paging in Phase 4.
Those are pure functions with real edge cases, and they are the parts that break
quietly.

In my project this was forced rather than chosen: my driver *cannot be imported*
off-device, because the sequencer library it binds to only exists on the target
hardware. So everything that could possibly live outside the driver was pushed
into a plain library module with no hardware dependency. That module now carries
**271 unit tests that run on any machine with nothing plugged in**, while the
driver file itself is verified by little more than a syntax check.

Being unable to test on my laptop turned out to be the best structural constraint
the project had. Nothing here forces the same discipline, which is precisely why
it is worth adopting deliberately: a `logic` module that takes state in and
returns intents out, with `main.rs` reduced to transport.

---

## 6. Silence must explain itself

A surface that can produce silence must be able to say *why* it is silent.

Mine could not. A per-pattern play-chance of 0 emits no notes at all, had no
indication anywhere on the hardware, and read exactly like a hang. It cost me a
jam before I understood what I was looking at. Now that state draws the channel's
tab with a dashed outline instead of a solid one — visibly "present but not
sounding", distinguishable at a glance from both "playing" and "absent".

The analogues here: a track that is muted, not armed, has no device, or is
outside the current bank. Any state where the hardware looks normal and produces
nothing deserves a distinct visual, not an absence of one.

While I am on rendering, one small mono-display trap that may generalise to the
canvas layouts: **do not fill a rectangle and then draw inverted text into it.**
The two operations cancel and you get a blank box. Draw the text normally, then
invert the whole rectangle.

---

## 7. Latched versus momentary modifiers

Relevant to Phase 3 and to `SHIFT` generally. `src/main.rs:187-190` tracks
`shift_pressed` as pure momentary state, which is correct for `SHIFT`.

For mode buttons — `SOLO`, `MUTE`, `PAD MODE`, `KEYBOARD` — the behaviour NI's
own firmware uses, which I settled by observation on hardware rather than from
any documentation, is:

- **Hold the button and press something else** → momentary. The action applies,
  and releasing the mode button returns to the previous mode. Additive: holding
  and pressing several targets affects all of them.
- **Tap the button alone** → latch into that mode and stay there.
- **Tap it again** → exit back to the previous mode.

This is worth implementing once, generically, rather than per button. The
distinction between the two gestures is simply whether any other button was
pressed before the mode button was released.

One measured trap on the host side, from the same feature: the mixer's
solo-toggle call in my host was **additive, not exclusive**, and had a special
case on the last channel index that cleared every solo. I would check Bitwig's
solo semantics against the same two questions before wiring the button.

---

## 8. Recording played notes over a pattern the machine also writes

This one is only relevant if Phase 3 ever acquires a generative source; if the
step sequencer is purely user-entered, most of it does not apply. I include it
because the two mechanical details underneath it are useful regardless.

**The two mechanical details, useful in any step sequencer:**

- **Quantise a strike to the *nearest* step, wrapping past the end of the
  pattern back to step 0.** Nearest, not previous. Rounding down turns every
  slightly-early hit into a late one, and it is audible immediately. Wrapping is
  not a delay, because the loop wraps within one step anyway.
- **A note's length is how long the pad was actually held**, clamped to the
  remaining space in the pattern. Fixed-length recorded notes feel wrong the
  moment anything sustains.

**The ownership problem, if a generator exists.** Once both the machine and the
player can write the same pattern, "who owns this pattern right now" becomes a
real piece of state, and it must be durable — it has to survive being saved and
reloaded. Two things I got wrong:

- I first tried to reuse an existing inter-thread mutex token as the ownership
  flag. It clears itself after every write, so it could not carry an ownership
  that outlives a single operation. Ownership needed its own state, saved in its
  own place.
- Handing the pattern back to the generator has to be **destructive and
  explicit** — an erase gesture, or turning a knob that necessarily rewrites the
  whole pattern. Critically, the knob route must fire only after a *real* value
  change is known. My first version fired on the arrival of any control message,
  so merely brushing an encoder without moving it a single unit destroyed a
  take, with no value having changed anywhere.

**One testing trap that wasted a lot of my time**, and which generalises to any
drum-oriented surface: **do not test note-off handling on a drum sound.** A
one-shot sample plays to the end whether or not the note-off ever arrives, so the
test cannot distinguish a correctly released note from a stuck one — it passes
either way. Test on a sustaining sound, where a held note stands indefinitely and
a released one decays. That difference is the only actual proof.

---

## Things that do not transfer

For completeness, so nothing above is mistaken for a broader claim: everything
device-specific in my project is useless here. The MK2's displays are two 255x64
**1-bit monochrome** panels addressed as eight full-width 8-row bands over HID
reports on the control interface — no relation to the Mk3's 480x272 BGR565 over a
bulk endpoint on a separate USB interface. Same for the LED byte offsets, the pad
velocity curve, and a kernel `hidraw` fault specific to the MK2 that I work
around with a close-and-reopen watchdog.

I did look at whether `encdr`'s frame-diffing could help my display path and
concluded it could not: my watchdog fires on *input* silence, not write volume,
so cutting USB writes buys nothing measurable. Mentioned only because it is the
obvious thing to assume in the other direction too.

---

## Closing

Good project. The decision to move rendering out of the host's sandbox into a
native process is the right one and the screenshots show it. Items **2** and
**4** are the two I would act on first, because they are present in the code
today rather than waiting in the roadmap, and both fail quietly rather than
loudly.

Happy to expand on any of these, or to be told they do not apply — I am working
from a read of the repository, not from running it.

**Reference:** <https://github.com/Witzman/Generative-Techno-ZynthianMaschine-MKII>
— Maschine MK2 + Zynthian, Rust HID daemon (`daemon/`) plus a Python
control-surface driver (`ctrldev/`). GPL-3.0.

If a concrete reading is more useful than a description, these are the exact
places the items above are implemented:

| Item | Where |
|---|---|
| **1** — page rings and the shape model | `ctrldev/techno_lib.py` — `PAGE_RINGS` table, `page_desc(shape, …)`, and the `SHAPE_SPREAD` / `SHAPE_GLOBAL` branches of the render path. `generated_pages()` is the "build pages from whatever the plugin publishes" part |
| **2** — re-read host state on load | `ctrldev/zynthian_ctrldev_maschine_mk2.py` — `_derive_params()`, wired to the `SS_LOAD_SNAPSHOT` signal. The play-chance/swing read-back described above is inside it |
| **4** — defer slow work off the event thread | `ctrldev/zynthian_ctrldev_maschine_mk2.py` — `_commit_kit()` and `_commit_preset()`. Both are called from a poll thread; the MIDI handler only records the intent |
| **5** — testable logic module | `ctrldev/techno_lib.py` + `ctrldev/maschine_mk2_lib.py`, tested by `ctrldev/tests/`. These run with no hardware and no Zynthian present |
| **8** — ownership and pattern handback | `ctrldev/zynthian_ctrldev_maschine_mk2.py` — `_write_voice_pattern()` returns early when the player owns the pattern |
