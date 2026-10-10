<script lang="ts">
  /**
   * Dev-only preview of the dual-screen UI with mock Bitwig data.
   * Run `npm run dev` and open /preview.html.
   *
   * Keys:  ← → (or mouse wheel) prev/next · ↑ parent (and close it) ·
   *        ↓ open group / enter first child · Enter toggle group · M master
   */
  import App from './App.svelte';
  import { state as appState } from './store.svelte';
  import type { TrackState } from './store.svelte';

  type Mock = { name: string; color: string; children?: Mock[]; open?: boolean; off?: boolean };
  const tree: Mock[] = [
    { name: 'Original Track', color: '#5e8ed6' },
    { name: 'Warped 80BPM', color: '#c9a03a', off: true },
    { name: '1_Wizzed 808BPM_(Other)', color: '#c9a03a' },
    { name: 'Full Vocal', color: '#9b5ad8', off: true },
    { name: 'Audio 5', color: '#e06440' },
    {
      name: 'Perc', color: '#8a8a94', open: true, children: [
        { name: 'Chopped Beat', color: '#d65a5a' },
        {
          name: 'Group 2', color: '#7a6ad8', open: true, children: [
            { name: 'Hats', color: '#a85ad8' },
            { name: 'Crash', color: '#d85aa0' },
            { name: 'Group 3', color: '#3fb7a5', open: true, children: [
              { name: 'Ride', color: '#5ad8c4' },
              { name: 'Shaker', color: '#8ad85a' },
            ] },
          ],
        },
        { name: 'Rev Crash', color: '#e8443a' },
        { name: 'Loop01_Niche_', color: '#e87a3a' },
      ],
    },
    { name: 'Vocal', color: '#9b5ad8' },
    { name: 'Breakdown Vocal', color: '#8a4ac8' },
    { name: 'Vocal Rev', color: '#b04ad0' },
    { name: 'Strings', color: '#5a7ad8' },
    { name: 'SBH_Bass_Loop_120_bpm_40', color: '#3a9a6a' },
    { name: 'Audio 14', color: '#c84a8a' },
    { name: 'FX Bus', color: '#d8b43a', open: false, children: [
      { name: 'Reverb', color: '#d8c45a' },
      { name: 'Delay', color: '#d8d45a' },
    ] },
    { name: 'Twinkle Arp', color: '#d84ab0' },
    { name: 'Sax', color: '#c03ad0' },
    { name: 'Bass', color: '#d04a6a' },
    { name: 'Bass Reverse', color: '#e05a4a' },
    { name: '808 Bass Lover', color: '#c8b03a' },
    { name: 'Dub Distorted Vocal', color: '#d0a03a' },
  ];

  // Flatten depth-first, like Bitwig's main track bank.
  const flat: TrackState[] = [];
  const walk = (nodes: Mock[], depth: number) => {
    for (const n of nodes) {
      flat.push({
        index: flat.length, name: n.name, color: n.color, volume: 0.7, volumeDisplay: '-3.2 dB',
        pan: 0.5, panDisplay: 'C', meter: 0, muted: false, solo: false, arm: false, exists: true,
        activated: !n.off, selected: false, isGroup: !!n.children, isGroupExpanded: !!n.open, depth,
      });
      if (n.children) walk(n.children, depth + 1);
    }
  };
  walk(tree, 0);
  while (flat.length < 32) {
    flat.push({ ...flat[0], index: flat.length, name: '', exists: false, isGroup: false, isGroupExpanded: false, depth: 0 });
  }

  let sel = $state(6); // Chopped Beat
  let master = $state(false);
  let tracks = $state(flat);

  // ── navigation (mirrors src/nav.rs) ─────────────────────────────
  function visible(): boolean[] {
    let hideBelow: number | null = null; // depth of the collapsed group currently hiding tracks
    return tracks.map((t) => {
      const d = t.depth ?? 0;
      if (!t.exists) return false;
      if (hideBelow !== null && d > hideBelow) return false;
      if (t.activated === false) { hideBelow = d; return false; }
      hideBelow = t.isGroup && !t.isGroupExpanded ? d : null;
      return true;
    });
  }
  const parentOf = (i: number) => {
    const d = tracks[i].depth ?? 0;
    if (d === 0) return -1;
    for (let j = i - 1; j >= 0; j--) if ((tracks[j].depth ?? 0) === d - 1) return j;
    return -1;
  };
  const firstChild = (i: number) =>
    i + 1 < tracks.length && tracks[i + 1].exists && (tracks[i + 1].depth ?? 0) === (tracks[i].depth ?? 0) + 1 ? i + 1 : -1;
  const step = (dir: 1 | -1) => {
    const v = visible();
    for (let j = sel + dir; j >= 0 && j < tracks.length; j += dir) if (v[j]) return j;
    return -1;
  };
  type Targets = { left: number; right: number; up: number; down: number };
  const targets = $derived.by((): Targets => {
    void tracks; void sel;
    const t = tracks[sel];
    const p = parentOf(sel);
    const up = p >= 0 ? p : t.isGroup && t.isGroupExpanded ? sel : -1;
    const down = t.isGroup ? (firstChild(sel) >= 0 ? firstChild(sel) : sel) : -1;
    return { left: step(-1), right: step(1), up, down };
  });

  function go(dir: 'left' | 'right' | 'up' | 'down') {
    appState.navSeq++;
    if (master) { master = false; return; }
    const t = tracks[sel];
    if (dir === 'left' || dir === 'right') {
      const n = step(dir === 'left' ? -1 : 1);
      if (n >= 0) sel = n;
    } else if (dir === 'up') {
      const p = parentOf(sel);
      if (p >= 0) { sel = p; tracks[p].isGroupExpanded = false; }
      else if (t.isGroup && t.isGroupExpanded) t.isGroupExpanded = false;
    } else if (dir === 'down' && t.isGroup) {
      t.isGroupExpanded = true;
      const c = firstChild(sel);
      if (c >= 0) sel = c;
    }
  }

  function onKey(e: KeyboardEvent) {
    const map: Record<string, 'left' | 'right' | 'up' | 'down'> = {
      ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down',
    };
    if (map[e.key]) { e.preventDefault(); go(map[e.key]); }
    else if (e.key === 'Enter' && tracks[sel].isGroup) tracks[sel].isGroupExpanded = !tracks[sel].isGroupExpanded;
    else if (e.key === 'm' || e.key === 'M') master = !master;
  }
  function onWheel(e: WheelEvent) { go(e.deltaY > 0 ? 'right' : 'left'); }

  // Push mock state into the shared store.
  $effect(() => {
    appState.page = 'channel';
    const mock = tracks.map((t) => ({ ...t, selected: !master && t.index === sel }));
    appState.tracks = mock;
    appState.strip = mock;
    appState.stripSelectedIndex = sel;
    const t = tracks[sel];
    appState.selectedTrackIndex = sel;
    appState.selectedTrackName = master ? 'Master' : t.name;
    appState.selectedTrackColor = master ? '#4a90e2' : t.color;
    appState.channelIsGroup = !master && !!t.isGroup;
    appState.channelIsGroupExpanded = !master && !!t.isGroupExpanded;
    appState.channelIsMaster = master;
    appState.selectedTrackVolume = 0.7;
    appState.selectedTrackVolumeDisplay = '-3.2 dB';
    ['Reverb', 'Raum Dub', 'Raum 1/2 Delay', 'Double Delay', 'Tape Delay'].forEach((name, i) => {
      Object.assign(appState.sends[i], { name, value: [0.62, 0, 0.35, 0, 0.2][i], display: ['-6.1 dB', '-inf dB', '-14.0 dB', '-inf dB', '-21.3 dB'][i] });
    });
    appState.sendsTotal = 5;
  });

  const led = (i: number) => (master ? '#4a90e2' : i >= 0 ? tracks[i].color : '#1b1b20');
</script>

<svelte:window onkeydown={onKey} />

<div class="wrap">
  <div class="screens" onwheel={onWheel}>
    <App screen="dual" />
  </div>
  <div class="controls">
    <div class="compass">
      <span class="led up" style="background:{led(targets.up)}"></span>
      <span class="led left" style="background:{led(targets.left)}"></span>
      <span class="knob"></span>
      <span class="led right" style="background:{led(targets.right)}"></span>
      <span class="led down" style="background:{led(targets.down)}"></span>
    </div>
    <p>← → / wheel: prev/next · ↑ parent (close) · ↓ open + enter · Enter: toggle group · M: master</p>
  </div>
</div>

<style>
  :global(body) { height: auto !important; overflow: auto !important; background: #050506 !important; }
  .wrap { padding: 24px; display: flex; flex-direction: column; gap: 20px; align-items: flex-start; }
  .screens { width: 960px; height: 272px; outline: 1px solid #222; position: relative; }
  .screens::after { content: ''; position: absolute; left: 479px; top: 0; bottom: 0; width: 2px; background: #000; opacity: .6; pointer-events: none; }
  .controls { display: flex; align-items: center; gap: 24px; color: #888; font: 13px system-ui; }
  .compass { display: grid; grid-template-columns: 18px 44px 18px; grid-template-rows: 18px 44px 18px; gap: 4px; place-items: center; }
  .led { width: 12px; height: 12px; border-radius: 50%; box-shadow: 0 0 8px currentColor; }
  .up { grid-column: 2; grid-row: 1; } .left { grid-column: 1; grid-row: 2; }
  .right { grid-column: 3; grid-row: 2; } .down { grid-column: 2; grid-row: 3; }
  .knob { grid-column: 2; grid-row: 2; width: 44px; height: 44px; border-radius: 50%; background: radial-gradient(#333, #111); border: 1px solid #444; }
</style>
