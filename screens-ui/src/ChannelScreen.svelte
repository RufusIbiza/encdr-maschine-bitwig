<script lang="ts">
  import { state as appState } from './store.svelte';
  import { PALETTE } from './palette';
  import ChannelStrip from './ChannelStrip.svelte';
  import ScrollIndicator from './ScrollIndicator.svelte';

  /*
   * The channel page is one 960x272 element spanning both hardware screens
   * (encdr's dual-screen view). Everything is laid out on 8 columns of 120px,
   * one above each encoder / screen button.
   */

  // Volume & Meter calculations
  let volPct = $derived(Math.max(0, Math.min(100, (appState.selectedTrackVolume || 0) * 100)));
  let meterRaw = $derived(appState.selectedTrackMeter || 0);

  // Stereo meters: if separate L/R not provided, simulate subtle stereo spread
  let meterPctL = $derived(Math.max(0, Math.min(100, (appState.selectedTrackMeterL !== undefined ? appState.selectedTrackMeterL : meterRaw) * 100)));
  let meterPctR = $derived(Math.max(0, Math.min(100, (appState.selectedTrackMeterR !== undefined ? appState.selectedTrackMeterR : meterRaw) * 100)));

  // Max channel level seen (follows Bitwig / resets on Shift + Volume)
  let peakDbDisplay = $derived.by(() => {
    const m = appState.selectedTrackMaxPeak !== undefined ? appState.selectedTrackMaxPeak : Math.max(meterPctL, meterPctR) / 100.0;
    if (m <= 0.0001) return '-∞';
    const db = 20 * Math.log10(m);
    if (db >= 0) return `+${db.toFixed(1)}`;
    return db.toFixed(1);
  });

  // Pan calculations for Attachment 2 slider
  let panVal = $derived(Math.max(0, Math.min(1, appState.selectedTrackPan !== undefined ? appState.selectedTrackPan : 0.5)));
  // Pan is centered at 0.5 (50%)
  let panLeftPct = $derived(Math.round(panVal * 100));
  let panFillStart = $derived(panVal < 0.5 ? panLeftPct : 50);
  let panFillWidth = $derived(Math.abs(panVal - 0.5) * 100);

  // Sends pagination calculations
  let totalSendsPages = $derived(Math.max(1, Math.ceil((appState.sendsTotal || 6) / 6)));
  let currentSendsPage = $derived((appState.sendsPage || 0) + 1);
  let canPrevSends = $derived((appState.sendsPage || 0) > 0);
  let canNextSends = $derived((appState.sendsPage || 0) + 1 < totalSendsPages);

  // Knob Dial Arc functions: 135 deg (Bottom-Left, 7:30) to 405 deg (Bottom-Right, 4:30)
  function describeArc(cx: number, cy: number, r: number, startAngle: number, endAngle: number) {
    const start = polarToCartesian(cx, cy, r, startAngle);
    const end = polarToCartesian(cx, cy, r, endAngle);
    const largeArcFlag = endAngle - startAngle <= 180 ? '0' : '1';
    return ['M', start.x, start.y, 'A', r, r, 0, largeArcFlag, 1, end.x, end.y].join(' ');
  }

  function polarToCartesian(cx: number, cy: number, r: number, angleInDegrees: number) {
    const angleInRadians = (angleInDegrees * Math.PI) / 180.0;
    return {
      x: cx + r * Math.cos(angleInRadians),
      y: cy + r * Math.sin(angleInRadians),
    };
  }

  const ARC_START = 135;
  const ARC_END = 405;
  const ARC_RANGE = ARC_END - ARC_START;

  // Monitor mode normalization: IN, AUTO, OUT
  let monitorModeRaw = $derived((appState.channelMonitorMode || 'AUTO').toUpperCase());
  let monitorDisplayMode = $derived.by(() => {
    if (monitorModeRaw === 'IN' || monitorModeRaw === 'ON') return 'IN';
    if (monitorModeRaw === 'OFF' || monitorModeRaw === 'OUT') return 'OUT';
    return 'AUTO';
  });

  // Monitor popover preview visibility
  let showMonitorMenu = $state(false);
  let monitorTimer: any = null;
  let lastMonMode = $state('');

  $effect(() => {
    const cur = monitorDisplayMode;
    if (lastMonMode !== '' && cur !== lastMonMode) {
      showMonitorMenu = true;
      if (monitorTimer) clearTimeout(monitorTimer);
      monitorTimer = setTimeout(() => {
        showMonitorMenu = false;
      }, 2500);
    }
    lastMonMode = cur;
  });

  function triggerMonitorMenu() {
    showMonitorMenu = true;
    if (monitorTimer) clearTimeout(monitorTimer);
    monitorTimer = setTimeout(() => {
      showMonitorMenu = false;
    }, 2500);
  }

  // Upper screen button labels:
  // Left:
  // Button 1: REC ARM
  // Button 2: —
  // Button 3: —
  // Button 4: MONITOR (IN / AUTO / OUT)
  // Right:
  // Button 5: < PAGE (Sends Page Prev)
  // Button 6: PAGE > (Sends Page Next)
  // Button 7: —
  // Button 8: MASTER (toggles master channel)
  let upperButtonLegends = $derived([
    { text: 'REC ARM', active: appState.selectedTrackArm, color: PALETTE.arm },
    { text: '—', active: false, color: '' },
    { text: '—', active: false, color: '' },
    { text: `MONITOR (${monitorDisplayMode})`, active: true, color: PALETTE.orange },
    { text: '< PAGE', active: canPrevSends, color: canPrevSends ? PALETTE.orange : '' },
    { text: 'PAGE >', active: canNextSends, color: canNextSends ? PALETTE.orange : '' },
    { text: '—', active: false, color: '' },
    { text: 'MASTER', active: appState.channelIsMaster, color: '#4a90e2' },
  ]);

  // Selected strip index. While the master is focused no strip track is
  // selected, so fall back to the last selected one.
  let stripSelectedIndex = $derived(
    appState.strip.find((t) => t.exists && t.selected)?.index ?? appState.stripSelectedIndex ?? -1
  );

  // Calculate perceived luminance to ensure maximum contrast (Bitwig aesthetic)
  function getContrastColor(hexColor: string): string {
    if (!hexColor || hexColor.length < 7) return '#ffffff';
    const r = parseInt(hexColor.slice(1, 3), 16) || 0;
    const g = parseInt(hexColor.slice(3, 5), 16) || 0;
    const b = parseInt(hexColor.slice(5, 7), 16) || 0;
    const yiq = (r * 299 + g * 587 + b * 114) / 1000;
    return yiq >= 145 ? '#111116' : '#ffffff';
  }

  // Active track color and contrast for Header & Selected Tab
  let activeTrackColor = $derived(
    appState.channelIsMaster
      ? (appState.masterColor || '#4a90e2')
      : (appState.selectedTrackColor || PALETTE.orange)
  );
  let activeContrastColor = $derived(getContrastColor(activeTrackColor));

  // Sends on the current page: six slots above encoders 3-8 (two on the left screen, four on the right).
  let baseSendIdx = $derived((appState.sendsPage || 0) * 6);
  let pageSends = $derived(
    Array.from({ length: 6 }, (_, i) => {
      const idx = baseSendIdx + i;
      return idx < (appState.sendsTotal || 0) ? appState.sends[idx] : null;
    })
  );
  let sendsPageLabel = $derived(totalSendsPages > 1 ? `${currentSendsPage}/${totalSendsPages}` : '');

  // Strip scroll window, reported by ChannelStrip for the scroll indicator.
  let stripView = $state({ first: 0, visible: 10, total: 0 });

  const VU_LEGEND_MARKS = ['+6', '0', '6', '12', '18', '24', '30', '36', '42', '48', '60', '∞'];
</script>

<div class="channel-view" style="--track-color: {activeTrackColor};">
  <!-- 1. Screen button legends (buttons 1-8) -->
  <div class="button-legends">
    {#each upperButtonLegends as btn, i}
      <button
        type="button"
        class="button-legend-item"
        class:is-active={btn.active}
        style={btn.active ? `color: ${btn.color}; border-top: 2px solid ${btn.color};` : ''}
        onclick={() => { if (i === 3) triggerMonitorMenu(); }}
      >
        <span>{btn.text}</span>
      </button>
    {/each}
    <ScrollIndicator
      {...stripView}
      touched={appState.navTouched}
      activity={appState.navSeq}
    />
  </div>

  <!-- Popover for button 4 (monitoring) -->
  {#if showMonitorMenu}
    <div class="monitor-popover-menu">
      <div class="monitor-popover-title">MONITORING</div>
      <div class="monitor-popover-item" class:is-active={monitorDisplayMode === 'IN'}>
        <span class="dot">●</span> IN
      </div>
      <div class="monitor-popover-item" class:is-active={monitorDisplayMode === 'AUTO'}>
        <span class="dot">●</span> AUTO
      </div>
      <div class="monitor-popover-item" class:is-active={monitorDisplayMode === 'OUT'}>
        <span class="dot">●</span> OUT
      </div>
    </div>
  {/if}

  <!-- 2. Channel strip -->
  <ChannelStrip
    tracks={appState.strip}
    selectedIndex={stripSelectedIndex}
    dimmed={appState.channelIsMaster}
    totalWidth={960}
    slotWidth={96}
    height={36}
    onscroll={(v) => (stripView = v)}
  />

  <!-- 3. Header: the selected strip tab merges into it -->
  <header
    class="header"
    style="background-color: {activeTrackColor}; color: {activeContrastColor};"
  >
    <div class="track-title-block">
      {#if appState.channelIsGroup}
        {#if appState.channelIsGroupExpanded}
          <svg class="folder-icon folder-open" viewBox="0 0 16 14" width="13" height="12" fill="{activeContrastColor}">
            <path opacity="0.6" d="M1 2a1 1 0 0 1 1-1h3.5a1 1 0 0 1 .7.3l1.5 1.7H13a1 1 0 0 1 1 1v2H2.5L1 12V2z"/>
            <path d="M2.5 6h12.8a.7.7 0 0 1 .68.9l-1.8 5.4a1 1 0 0 1-.95.7H1.8a.7.7 0 0 1-.68-.9l1.4-5.4a1 1 0 0 1 .98-.7z"/>
          </svg>
        {:else}
          <svg class="folder-icon folder-closed" viewBox="0 0 16 14" width="13" height="12" fill="{activeContrastColor}">
            <path d="M1 2a1 1 0 0 1 1-1h3.5a1 1 0 0 1 .7.3l1.5 1.7H14a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H2a1 1 0 0 1-1-1V2z"/>
          </svg>
        {/if}
      {/if}
      <span class="track-title" style="color: {activeContrastColor};">
        {appState.channelIsMaster ? (appState.masterName || 'MASTER').toUpperCase() : (appState.selectedTrackName.toUpperCase() || 'NO TRACK')}
      </span>
      {#if appState.channelIsGroup}
        <span
          class="group-header-badge"
          style="color: {activeContrastColor}; border-color: {activeContrastColor === '#ffffff' ? 'rgba(255,255,255,0.35)' : 'rgba(0,0,0,0.3)'}; background: {activeContrastColor === '#ffffff' ? 'rgba(255,255,255,0.18)' : 'rgba(0,0,0,0.12)'};"
        >
          {appState.channelIsGroupExpanded ? 'OPEN GROUP' : 'GROUP'}
        </span>
      {/if}
      {#if appState.channelIsMaster}
        <span
          class="master-header-badge"
          style="color: {activeContrastColor}; border-color: {activeContrastColor === '#ffffff' ? 'rgba(255,255,255,0.35)' : 'rgba(0,0,0,0.3)'}; background: {activeContrastColor === '#ffffff' ? 'rgba(255,255,255,0.18)' : 'rgba(0,0,0,0.12)'};"
        >
          MASTER
        </span>
      {/if}
    </div>
  </header>

  <!-- 4. Body: one 120px column per encoder -->
  <div class="channel-body">
    <!-- Encoder 1: volume fader + stereo meter, filling the column -->
    <div class="col-volume">
      <div class="bitwig-meter-card">
        <div class="meter-readouts-header">
          <span class="peak-readout">{peakDbDisplay}</span>
          <span class="fader-readout">{appState.selectedTrackVolumeDisplay || `${Math.round(volPct)}%`}</span>
        </div>
        <div class="meter-divider-line"></div>

        <div class="meter-stage">
          <div class="vu-legend">
            {#each VU_LEGEND_MARKS as mark}
              <div class="legend-tick-row">
                <span class="legend-num">{mark}</span>
                <span class="legend-tick">-</span>
              </div>
            {/each}
          </div>

          <div class="stereo-meter-bars">
            <div class="meter-bar-track">
              <div class="meter-bar-fill" style="height: {meterPctL}%;"></div>
              <div class="meter-bar-peak" style="bottom: {meterPctL}%;"></div>
            </div>
            <div class="meter-bar-track">
              <div class="meter-bar-fill" style="height: {meterPctR}%;"></div>
              <div class="meter-bar-peak" style="bottom: {meterPctR}%;"></div>
            </div>
          </div>

          <div class="fader-lane">
            <div class="fader-fill-dim" style="height: {volPct}%;">
              <div class="fader-top-border"></div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Encoder 2: arm / solo / mute status and pan -->
    <div class="col-pan">
      <div class="status-indicators-row">
        <div class="status-btn btn-arm" class:active={appState.selectedTrackArm} title="Arm">
          <span class="rec-icon">●</span>
        </div>
        <div class="status-btn btn-solo" class:active={appState.selectedTrackSolo} title="Solo">S</div>
        <div class="status-btn btn-mute" class:active={appState.selectedTrackMuted} title="Mute">M</div>
      </div>

      <div class="knob-section">
        <div class="knob-label">PAN</div>
        <div class="pan-slider-frame">
          <div class="pan-center-line"></div>
          {#if panFillWidth > 0.5}
            <div class="pan-highlight-fill" style="left: {panFillStart}%; width: {panFillWidth}%;"></div>
          {/if}
          <div class="pan-indicator-line" style="left: {panLeftPct}%;"></div>
        </div>
        <div class="pan-readout-val">{appState.selectedTrackPanDisplay || 'C'}</div>
      </div>
    </div>

    <!-- Encoders 3-8: sends, bracketed per screen (3-4 left, 5-8 right) -->
    <div class="sends-bracket left">
      <span class="bracket-label">SENDS{#if sendsPageLabel}<em>{sendsPageLabel}</em>{/if}</span>
    </div>
    <div class="sends-bracket right">
      <span class="bracket-label">SENDS{#if sendsPageLabel}<em>{sendsPageLabel}</em>{/if}</span>
    </div>

    {#each pageSends as send, i}
      {@const val = send ? Math.max(0, Math.min(1, send.value || 0)) : 0}
      <div class="knob-section send" class:empty={!send} style="grid-column: {i + 3};">
        <div class="knob-label" title={send?.name}>{send ? send.name : '—'}</div>
        <svg class="send-svg" viewBox="0 14 100 64">
          <path
            d={describeArc(50, 50, 32, ARC_START, ARC_END)}
            fill="none"
            stroke="#24242c"
            stroke-width="6"
            stroke-linecap="round"
          />
          {#if send && val > 0.005}
            <path
              d={describeArc(50, 50, 32, ARC_START, ARC_START + val * ARC_RANGE)}
              fill="none"
              stroke={PALETTE.orange}
              stroke-width="6"
              stroke-linecap="round"
            />
          {/if}
          {#if send}
            <text x="50" y="52" class="dial-readout" text-anchor="middle" dominant-baseline="middle">
              {send.display || `${Math.round(val * 100)}%`}
            </text>
          {/if}
        </svg>
      </div>
    {/each}
  </div>
</div>

<style>
  .channel-view {
    width: 960px;
    height: 272px;
    background: #141416;
    color: #e5e5e7;
    display: flex;
    flex-direction: column;
    user-select: none;
    overflow: hidden;
    position: relative;
  }

  /* 1. Screen button legends: 8 x 120px, one per hardware button */
  .button-legends {
    height: 24px;
    flex: 0 0 24px;
    background: #0d0d10;
    border-bottom: 1px solid #1f1f26;
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    position: relative;
    z-index: 20;
  }

  .button-legend-item {
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-right: 1px solid #1a1a22;
    font-size: 10px;
    font-weight: 700;
    color: #727282;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    cursor: pointer;
    padding: 0;
    outline: none;
    transition: background 0.15s ease;
  }

  /* no divider at the bezel or the right edge */
  .button-legend-item:nth-child(4),
  .button-legend-item:last-child {
    border-right: none;
  }

  .button-legend-item.is-active {
    background: rgba(255, 140, 26, 0.12);
  }

  /* Popover under button 4 */
  .monitor-popover-menu {
    position: absolute;
    top: 25px;
    left: 365px;
    width: 110px;
    background: #161620;
    border: 1px solid #ff8c1a;
    border-radius: 4px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.95), 0 0 10px rgba(255, 140, 26, 0.25);
    z-index: 100;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    animation: popoverFade 0.15s ease-out;
  }

  @keyframes popoverFade {
    from { opacity: 0; transform: translateY(-3px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .monitor-popover-title {
    font-size: 8px;
    font-weight: 800;
    color: #7d7d8e;
    letter-spacing: 0.8px;
    padding: 2px 4px;
    text-align: center;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .monitor-popover-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 3px;
    font-size: 9.5px;
    font-weight: 700;
    color: #727284;
    background: #111117;
  }

  .monitor-popover-item.is-active {
    background: #ff8c1a;
    color: #000000;
    font-weight: 800;
    box-shadow: 0 0 6px rgba(255, 140, 26, 0.5);
  }

  .monitor-popover-item .dot {
    font-size: 8px;
    opacity: 0.85;
  }

  .folder-icon {
    flex-shrink: 0;
    vertical-align: middle;
  }

  /* 3. Header (the selected strip tab merges into it with no line) */
  .header {
    height: 22px;
    flex: 0 0 22px;
    border-bottom: 1px solid rgba(0, 0, 0, 0.3);
    display: flex;
    align-items: center;
    padding: 0 12px;
    position: relative;
    z-index: 5;
    transition: background-color 0.2s ease;
  }

  .track-title-block {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .track-title {
    font-size: 12.5px;
    font-weight: 800;
    letter-spacing: 0.5px;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.8);
  }

  .group-header-badge,
  .master-header-badge {
    font-size: 9.5px;
    font-weight: 800;
    padding: 1px 6px;
    border-radius: 3px;
    border: 1px solid;
  }

  /* 4. Body: 8 encoder columns; sends sit at the bottom with their brackets just above */
  .channel-body {
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(8, 120px);
    grid-template-rows: 1fr auto auto;
    padding: 6px 0 3px;
  }

  .col-volume,
  .col-pan {
    grid-row: 1 / 4;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .col-volume {
    grid-column: 1;
    padding: 0 8px 0 10px;
  }

  .col-pan {
    grid-column: 2;
    align-items: center;
    justify-content: space-between;
    border-right: 1px solid #23232c;
  }

  /* Send brackets: ⸢── SENDS ──⸣ spanning the knobs on each screen */
  .sends-bracket {
    grid-row: 2;
    position: relative;
    margin: 0 16px 10px;
    border: 1.5px solid #4a4a58;
    border-bottom: none;
    border-radius: 3px 3px 0 0;
    height: 9px;
  }
  .sends-bracket.left {
    grid-column: 3 / 5;
  }
  .sends-bracket.right {
    grid-column: 5 / 9;
  }

  .bracket-label {
    position: absolute;
    left: 50%;
    top: -1px;
    transform: translate(-50%, -50%);
    background: #141416;
    padding: 0 8px;
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 1.5px;
    color: #8c8c9a;
    white-space: nowrap;
  }

  .bracket-label em {
    font-style: normal;
    font-weight: 700;
    color: #5f5f6c;
    margin-left: 6px;
    letter-spacing: 0.5px;
  }

  /* Knob sections: label above the control, bottom-aligned across columns */
  .knob-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    gap: 2px;
    min-width: 0;
  }

  .knob-section.send {
    grid-row: 3;
  }

  .knob-section.send.empty {
    opacity: 0.35;
  }

  .knob-label {
    font-size: 10.5px;
    font-weight: 700;
    color: #7e7e8c;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 110px;
    height: 14px;
    line-height: 14px;
  }

  /* Status switches [● Arm] [S Solo] [M Mute] */
  .status-indicators-row {
    width: 96px;
    height: 22px;
    margin-top: 14px;
    display: flex;
    gap: 4px;
  }

  .status-btn {
    flex: 1;
    height: 100%;
    background: #1b1b22;
    border: 1px solid #2b2b36;
    border-radius: 3px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 800;
    color: #5d5d6c;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05);
  }

  .rec-icon {
    font-size: 11px;
  }

  .btn-arm.active {
    background: #ff3b30;
    color: #ffffff;
    border-color: #ff3b30;
    box-shadow: 0 0 6px rgba(255, 59, 48, 0.5);
  }

  .btn-solo.active {
    background: #ffd11a;
    color: #000000;
    border-color: #ffd11a;
    box-shadow: 0 0 6px rgba(255, 209, 26, 0.5);
  }

  .btn-mute.active {
    background: #ff8c1a;
    color: #000000;
    border-color: #ff8c1a;
    box-shadow: 0 0 6px rgba(255, 140, 26, 0.5);
  }

  /* Volume fader + meter card: stretches to the full body height */
  .bitwig-meter-card {
    flex: 1;
    min-height: 0;
    background: #0f0f13;
    border: 1px solid #282834;
    border-radius: 4px;
    display: flex;
    flex-direction: column;
    padding: 3px 4px 4px;
  }

  .meter-readouts-header {
    height: 16px;
    flex: 0 0 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 2px;
  }

  .peak-readout {
    font-size: 10px;
    font-weight: 600;
    color: #888894;
    font-family: monospace;
  }

  .fader-readout {
    font-size: 11px;
    font-weight: 700;
    color: #ff8c1a;
    font-family: monospace;
  }

  .meter-divider-line {
    height: 1px;
    flex: 0 0 1px;
    background: #242430;
    margin: 2px 0 3px;
  }

  .meter-stage {
    flex: 1;
    min-height: 0;
    display: flex;
    position: relative;
    overflow: hidden;
    background: #111116;
    border-radius: 2px;
  }

  .vu-legend {
    width: 22px;
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 2px 0;
  }

  .legend-tick-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 1px;
    line-height: 1;
  }

  .legend-num {
    font-size: 7.5px;
    font-weight: 600;
    color: #656574;
    font-family: monospace;
  }

  .legend-tick {
    font-size: 7.5px;
    color: #4a4a58;
  }

  .stereo-meter-bars {
    width: 14px;
    height: 100%;
    display: flex;
    justify-content: center;
    gap: 2px;
    padding: 2px 0;
    z-index: 1;
  }

  .meter-bar-track {
    width: 4px;
    height: 100%;
    background: #181822;
    border-radius: 1px;
    position: relative;
    overflow: hidden;
  }

  .meter-bar-fill {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    background: linear-gradient(to top, #10b981 0%, #10b981 65%, #f59e0b 82%, #ef4444 100%);
    border-radius: 1px;
  }

  .meter-bar-peak {
    position: absolute;
    left: 0;
    right: 0;
    height: 1.5px;
    background: #ffffff;
    box-shadow: 0 0 2px #ffffff;
  }

  .fader-lane {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 38px;
    right: 2px;
    z-index: 2;
  }

  .fader-fill-dim {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    background: rgba(255, 140, 26, 0.3);
    border-radius: 1px;
  }

  .fader-top-border {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2.5px;
    background: #ff8c1a;
    box-shadow: 0 0 4px rgba(255, 140, 26, 0.8);
  }

  /* Pan slider */
  .pan-slider-frame {
    width: 92px;
    height: 22px;
    background: #111116;
    border: 1px solid #333342;
    border-radius: 3px;
    position: relative;
    overflow: hidden;
  }

  .pan-center-line {
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 50%;
    width: 1px;
    background: #444455;
    transform: translateX(-50%);
  }

  .pan-highlight-fill {
    position: absolute;
    top: 0;
    bottom: 0;
    background: rgba(255, 140, 26, 0.35);
  }

  .pan-indicator-line {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2.5px;
    background: #ff8c1a;
    transform: translateX(-50%);
    box-shadow: 0 0 4px rgba(255, 140, 26, 0.9);
  }

  .pan-readout-val {
    font-size: 13px;
    font-weight: 700;
    color: #ffffff;
    font-family: monospace;
    letter-spacing: 0.5px;
    height: 16px;
  }

  /* Send dials */
  /* viewBox is cropped to the arc, so the dial sits flush with the bottom */
  .send-svg {
    width: 92px;
    height: 59px;
    flex: 0 0 auto;
  }

  .dial-readout {
    font-size: 12px;
    font-weight: 700;
    fill: #ffffff;
    font-family: monospace;
  }
</style>
