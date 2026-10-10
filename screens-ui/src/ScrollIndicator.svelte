<!--
  ScrollIndicator — macOS-style overlay scrollbar for the channel strip.

  Appears while the 4D encoder is touched or used to navigate, and fades out
  one second after the last activity. The thumb spans the share of channels
  currently on screen, so it shrinks as groups open and the strip grows.
-->
<script lang="ts">
  interface Props {
    /** First visible slot, slots on screen, and total slots in the strip. */
    first: number;
    visible: number;
    total: number;
    /** Finger on the 4D encoder. */
    touched: boolean;
    /** Changes on every 4D-encoder navigation. */
    activity: number;
    width?: number;
  }

  let { first, visible, total, touched, activity, width = 960 }: Props = $props();

  const HOLD_MS = 1000;
  const FADE_MS = 250;
  const MOVE_MS = 320; // matches the strip's scroll animation
  const INSET = 8;
  const MIN_THUMB = 24;

  const trackW = $derived(width - INSET * 2);
  const thumbW = $derived(
    total <= visible ? trackW : Math.max(MIN_THUMB, Math.round((trackW * visible) / Math.max(1, total))),
  );
  const thumbX = $derived(
    total <= visible ? 0 : Math.round(((trackW - thumbW) * first) / Math.max(1, total - visible)),
  );

  let shown = $state(false);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  function holdThenHide() {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => (shown = false), HOLD_MS);
  }

  $effect(() => {
    if (touched) {
      shown = true;
      clearTimeout(hideTimer);
    } else if (shown) {
      holdThenHide();
    }
  });

  // Navigation (and the scroll it causes) keeps the indicator up, restarting
  // the timeout unless the encoder is still being touched.
  let seenActivity: number | undefined;
  $effect(() => {
    void first;
    void total;
    const navigated = seenActivity !== undefined && activity !== seenActivity;
    seenActivity = activity;
    if (navigated) shown = true;
    if (shown && !touched) holdThenHide();
  });

  $effect(() => () => clearTimeout(hideTimer));

  // The hardware screens are captured on demand; keep requesting frames
  // while the fade or thumb movement animates.
  let animUntil = 0;
  $effect(() => {
    void shown;
    void thumbX;
    void thumbW;
    if (typeof window === 'undefined') return;
    const running = performance.now() < animUntil;
    animUntil = performance.now() + Math.max(FADE_MS, MOVE_MS) + 60;
    if (running) return;
    const tick = () => {
      window.encdr?.requestCapture?.();
      if (performance.now() < animUntil) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
</script>

<div
  class="scroll-indicator"
  class:shown
  style="--inset: {INSET}px; --fade: {FADE_MS}ms; --move: {MOVE_MS}ms;"
  aria-hidden="true"
>
  <div class="track">
    <div class="thumb" style="width: {thumbW}px; transform: translate3d({thumbX}px, 0, 0);"></div>
  </div>
</div>

<style>
  .scroll-indicator {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    padding: 0 var(--inset);
    background: rgba(13, 13, 16, 0.93);
    opacity: 0;
    transition: opacity var(--fade) ease;
    pointer-events: none;
    z-index: 30;
  }

  .scroll-indicator.shown {
    opacity: 1;
  }

  .track {
    position: relative;
    width: 100%;
    height: 10px;
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.07);
  }

  .thumb {
    position: absolute;
    top: 1px;
    left: 0;
    height: 8px;
    border-radius: 4px;
    background: rgba(236, 236, 242, 0.62);
    box-shadow: 0 0 0 0.5px rgba(0, 0, 0, 0.5);
    transition:
      transform var(--move) cubic-bezier(0.22, 1, 0.36, 1),
      width var(--move) cubic-bezier(0.22, 1, 0.36, 1);
  }
</style>
