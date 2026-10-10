/**
 * Pure layout model for the channel strip.
 *
 * Bitwig's main track bank is a flat, arranger-ordered (depth-first) list of
 * tracks. Each track carries its nesting `depth` (0 = top level), so the
 * hierarchy can be rebuilt without relying on track names:
 *   - the parent of track i is the nearest preceding track with depth - 1
 *   - a track is visible only if every ancestor group is expanded
 *
 * The model works for any nesting depth and also copes with the bank being
 * scrolled into the middle of a group (ancestors that are not in the bank are
 * synthesised as "virtual" groups whose bracket runs off the left edge).
 */

export interface StripTrack {
  index: number;
  name: string;
  color: string;
  exists: boolean;
  /** Deactivated tracks (and a deactivated group's contents) are hidden, as in Bitwig. */
  activated?: boolean;
  isGroup?: boolean;
  isGroupExpanded?: boolean;
  depth?: number;
  /** Fallback colours used when an ancestor is outside the bank window. */
  parentColor?: string;
  grandparentColor?: string;
}

export interface StripItem {
  track: StripTrack;
  /** Bank index of the track. */
  index: number;
  /** Position in the visible ribbon (0-based). */
  slot: number;
  depth: number;
  isGroup: boolean;
  expanded: boolean;
  color: string;
  fg: string;
}

/** The coloured bracket drawn over the children of an open group. */
export interface StripBand {
  key: string;
  /** Bank index of the owning group, or -1 for a virtual (off-bank) group. */
  groupIndex: number;
  depth: number;
  color: string;
  startSlot: number;
  endSlot: number;
  /** True when the group starts before the bank window (no left cap). */
  openStart: boolean;
}

export interface StripLayout {
  items: StripItem[];
  bands: StripBand[];
  /** Bank index -> visible slot. Hidden tracks map to their visible (collapsed) ancestor. */
  slotOf: Map<number, number>;
  maxDepth: number;
}

interface Frame {
  depth: number;
  groupIndex: number;
  color: string;
  expanded: boolean;
  /** Slot of the group's own tab (-1 for virtual groups or hidden groups). */
  slot: number;
  firstChildSlot: number;
  lastSlot: number;
  hidden: boolean;
  virtual: boolean;
}

const FALLBACK_GROUP_COLOR = '#5c5c68';

export function contrastText(hex: string): string {
  const c = (hex || '').replace('#', '');
  if (c.length < 6) return '#f4f4f6';
  const r = parseInt(c.slice(0, 2), 16) || 0;
  const g = parseInt(c.slice(2, 4), 16) || 0;
  const b = parseInt(c.slice(4, 6), 16) || 0;
  const yiq = (r * 299 + g * 587 + b * 114) / 1000;
  return yiq >= 150 ? '#141416' : '#f6f6f8';
}

export function buildStripLayout(tracks: StripTrack[]): StripLayout {
  const items: StripItem[] = [];
  const bands: StripBand[] = [];
  const slotOf = new Map<number, number>();
  const stack: Frame[] = [];
  let slot = 0;
  let maxDepth = 0;

  const close = (f: Frame) => {
    if (f.hidden || !f.expanded || f.firstChildSlot < 0) return;
    bands.push({
      key: f.virtual ? `v${f.depth}-${f.firstChildSlot}` : `g${f.groupIndex}`,
      groupIndex: f.virtual ? -1 : f.groupIndex,
      depth: f.depth,
      color: f.color,
      startSlot: f.virtual ? f.firstChildSlot : f.slot,
      endSlot: f.lastSlot,
      openStart: f.virtual,
    });
  };

  const ordered = tracks.filter((t) => t && t.exists).sort((a, b) => a.index - b.index);

  for (const t of ordered) {
    const depth = Math.max(0, Math.floor(t.depth ?? 0));

    // Leave any groups this track is not inside of.
    while (stack.length && stack[stack.length - 1].depth >= depth) close(stack.pop()!);

    // Synthesise ancestors that are outside the bank window.
    while (stack.length < depth) {
      const level = stack.length;
      const distance = depth - 1 - level; // 0 = direct parent
      const color =
        (distance === 0 ? t.parentColor : distance === 1 ? t.grandparentColor : '') || FALLBACK_GROUP_COLOR;
      stack.push({
        depth: level,
        groupIndex: -1,
        color,
        expanded: true,
        slot: -1,
        firstChildSlot: -1,
        lastSlot: -1,
        hidden: stack.some((f) => f.hidden || !f.expanded),
        virtual: true,
      });
    }

    const hidden = stack.some((f) => f.hidden || !f.expanded) || t.activated === false;

    if (hidden) {
      // Map to the outermost collapsed ancestor that is itself visible.
      const proxy = stack.find((f) => !f.hidden && !f.expanded);
      if (proxy && proxy.slot >= 0) slotOf.set(t.index, proxy.slot);
    } else {
      const s = slot++;
      slotOf.set(t.index, s);
      maxDepth = Math.max(maxDepth, depth);
      for (const f of stack) {
        if (f.firstChildSlot < 0) f.firstChildSlot = s;
        f.lastSlot = s;
      }
      items.push({
        track: t,
        index: t.index,
        slot: s,
        depth,
        isGroup: !!t.isGroup,
        expanded: !!t.isGroupExpanded,
        color: t.color || '#888892',
        fg: contrastText(t.color || '#888892'),
      });
    }

    if (t.isGroup) {
      stack.push({
        depth,
        groupIndex: t.index,
        color: t.color || FALLBACK_GROUP_COLOR,
        expanded: !!t.isGroupExpanded,
        slot: hidden ? -1 : slotOf.get(t.index)!,
        firstChildSlot: -1,
        lastSlot: -1,
        hidden,
        virtual: false,
      });
    }
  }
  while (stack.length) close(stack.pop()!);

  // Outer bands first so nested bands paint on top of them.
  bands.sort((a, b) => a.depth - b.depth);
  return { items, bands, slotOf, maxDepth };
}

/**
 * Returns the first visible slot so that `selected` stays in view with `margin`
 * slots of context on either side, scrolling as little as possible.
 */
export function keepInView(prevFirst: number, selected: number, total: number, visible: number, margin: number): number {
  const maxFirst = Math.max(0, total - visible);
  let first = Math.min(Math.max(0, prevFirst), maxFirst);
  if (selected < 0) return first;
  const m = Math.min(margin, Math.floor((visible - 1) / 2));
  if (selected < first + m) first = selected - m;
  else if (selected > first + visible - 1 - m) first = selected - visible + 1 + m;
  return Math.min(Math.max(0, first), maxFirst);
}
