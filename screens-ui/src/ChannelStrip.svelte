<!--
  ChannelStrip — Bitwig-style horizontal track ribbon.

  Renders one continuous ribbon across both hardware screens (`totalWidth`),
  scrolling as little as possible to keep the selected track in view.

  Visual language:
    - every track is a fixed-width tab in its own colour with contrasting text
    - the selected tab drops down into the header row below (no border)
    - an open group's colour continues as a bar over its children: the group tab
      and bar share a rounded top-left corner, and the bar ends with a rounded
      corner flush with its last child; nested groups stack one level lower
    - groups show a chevron pointing the way the 4D encoder enters/leaves them
      (▾ closed: push down to open; ▸ open)
-->
<script lang="ts">
  import { buildStripLayout, keepInView, type StripTrack } from './channelStrip';

  interface Props {
    tracks: StripTrack[];
    /** Bank index of the selected track (-1 for none). */
    selectedIndex: number;
    /** Dim the whole strip (e.g. while the master channel is focused). */
    dimmed?: boolean;
    /** Total width the strip spans across all screens. */
    totalWidth?: number;
    /** Fixed width of a channel slot. Use a divisor of 480 so no tab straddles the screen bezel. */
    slotWidth?: number;
    height?: number;
    /** Reports the scroll window in slots whenever it changes (for a scroll indicator). */
    onscroll?: (view: { first: number; visible: number; total: number }) => void;
  }

  let {
    tracks,
    selectedIndex,
    dimmed = false,
    totalWidth = 960,
    slotWidth = 96,
    height = 36,
    onscroll,
  }: Props = $props();

  const FLOOR = 3; // gap under unselected tabs; the selected tab fills it
  const MIN_TAB = 15; // smallest tab height, reached only at the deepest nesting
  const RADIUS = 4; // top corners of tabs and group bars
  const GAP = 2; // horizontal gap between tabs
  const ANIM_MS = 320;

  const layout = $derived(buildStripLayout(tracks));
  // Vertical step per nesting level: as bold as the space allows, shrinking
  // only when deep nesting would squash the innermost tabs.
  const level = $derived(
    Math.max(4, Math.min(7, Math.floor((height - FLOOR - MIN_TAB) / Math.max(1, layout.maxDepth)))),
  );
  const band = $derived(level - 1); // bracket thickness; 1px gap to the next level
  const selectedSlot = $derived(layout.slotOf.get(selectedIndex) ?? -1);
  const visibleCount = $derived(Math.max(1, Math.floor(totalWidth / slotWidth)));

  // Scroll position is path dependent (scroll as little as possible), so keep
  // the previous value in a plain variable that the derivation feeds from.
  let prevFirst = 0;
  const firstSlot = $derived.by(() => {
    const f = keepInView(prevFirst, selectedSlot, layout.items.length, visibleCount, visibleCount >= 4 ? 1 : 0);
    prevFirst = f;
    return f;
  });
  const offsetX = $derived(firstSlot * slotWidth);

  $effect(() => {
    onscroll?.({ first: firstSlot, visible: visibleCount, total: layout.items.length });
  });

  // The hardware screens are captured on demand; keep requesting frames while
  // CSS transitions run so the animation is visible on the device.
  let animUntil = 0;
  function pumpCaptures() {
    animUntil = performance.now() + ANIM_MS + 60;
    const tick = () => {
      window.encdr?.requestCapture?.();
      if (performance.now() < animUntil) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }
  $effect(() => {
    // dependencies that trigger transitions
    void firstSlot;
    void selectedIndex;
    void dimmed;
    if (typeof window !== 'undefined') pumpCaptures();
  });

  function lineClamp(depth: number): number {
    return height - FLOOR - depth * level >= 25 ? 2 : 1;
  }
</script>

<div
  class="strip"
  class:dimmed
  style="height: {height}px; --anim: {ANIM_MS}ms; --band: {band}px; --floor: {FLOOR}px; --radius: {RADIUS}px;"
>
  <div class="ribbon" style="transform: translate3d({-offsetX}px, 0, 0);">
    {#each layout.bands as b (b.key)}
      {@const left = b.openStart ? -GAP : b.startSlot * slotWidth + GAP / 2}
      {@const right = (b.endSlot + 1) * slotWidth - GAP / 2}
      <div
        class="band"
        class:open-start={b.openStart}
        class:active={b.groupIndex === selectedIndex && !dimmed}
        style="left: {left}px; width: {right - left}px; top: {b.depth * level}px; --c: {b.color};"
      ></div>
    {/each}

    {#each layout.items as item (item.index)}
      {@const selected = item.index === selectedIndex || (item.slot === selectedSlot && selectedSlot >= 0)}
      <div
        class="tab"
        class:selected
        class:merged={selected && !dimmed}
        class:group={item.isGroup}
        class:collapsed={item.isGroup && !item.expanded}
        style="left: {item.slot * slotWidth + GAP / 2}px; width: {slotWidth - GAP}px; top: {item.depth *
          level}px; --c: {item.color}; --fg: {item.fg}; --lines: {lineClamp(item.depth)};"
      >
        {#if item.isGroup}
          <svg class="chevron" viewBox="0 0 8 8" width="9" height="9" aria-hidden="true">
            <path d="M2 1 L6.5 4 L2 7 Z" />
          </svg>
        {/if}
        <span class="name">{item.track.name}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .strip {
    position: relative;
    width: 100%;
    overflow: hidden;
    background: #0e0e11;
    flex: 0 0 auto;
  }

  .ribbon {
    position: absolute;
    inset: 0;
    transition:
      transform var(--anim) cubic-bezier(0.22, 1, 0.36, 1),
      opacity 0.25s ease,
      filter 0.25s ease;
    will-change: transform;
  }

  .strip.dimmed .ribbon {
    opacity: 0.3;
    filter: saturate(0.55);
  }

  /* ── Group bar ─────────────────────────────────────────────────── */
  .band {
    position: absolute;
    height: var(--band);
    background: var(--c);
    border-radius: var(--radius) var(--radius) 0 0;
    /* same shading as the group's own tab, so tab and bar read as one shape */
    filter: brightness(0.8);
    transition: filter 0.2s ease;
  }
  .band.open-start {
    border-top-left-radius: 0;
  }
  .band.active {
    filter: none;
  }

  /* ── Track tab ─────────────────────────────────────────────────── */
  .tab {
    position: absolute;
    bottom: var(--floor);
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 0 5px;
    background-color: var(--c);
    /* subtle top sheen that fades to the flat colour, so a merged tab joins the header seamlessly */
    background-image: linear-gradient(to bottom, rgba(255, 255, 255, 0.14), rgba(255, 255, 255, 0) 70%);
    color: var(--fg);
    border-radius: var(--radius) var(--radius) 0 0;
    filter: brightness(0.8);
    transition:
      bottom 0.18s ease,
      filter 0.2s ease,
      top 0.2s ease;
  }

  .tab.selected {
    filter: none;
  }

  /* selected tab drops into the header row below */
  .tab.merged {
    bottom: 0;
    background-image: linear-gradient(to bottom, rgba(255, 255, 255, 0.2), rgba(255, 255, 255, 0) 55%);
  }

  .name {
    font-size: 11px;
    line-height: 12px;
    font-weight: 650;
    letter-spacing: 0.1px;
    text-align: center;
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: var(--lines);
    line-clamp: var(--lines);
    word-break: break-word;
  }

  .tab.selected .name {
    font-weight: 800;
  }

  .chevron {
    flex: 0 0 auto;
    fill: currentColor;
    opacity: 0.9;
    transition: transform 0.2s ease;
  }
  .tab.collapsed .chevron {
    transform: rotate(90deg);
  }
</style>
