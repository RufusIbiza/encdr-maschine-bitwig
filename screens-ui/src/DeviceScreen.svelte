<script lang="ts">
  import { state } from './store.svelte';
  import { PALETTE } from './palette';

  interface Props {
    screen: 'left' | 'right';
  }

  let { screen }: Props = $props();

  let startIndex = $derived(screen === 'left' ? 0 : 4);
  let params = $derived(
    Array.from({ length: 4 }, (_, i) => {
      const idx = startIndex + i;
      return {
        idx,
        name: state.names[idx] || '—',
        display: state.displays[idx] || (Math.round((state.values[idx] || 0) * 100) + '%'),
        value: Math.max(0, Math.min(1, state.values[idx] || 0)),
        modulated: Math.max(0, Math.min(1, state.modulated[idx] !== undefined ? state.modulated[idx] : (state.values[idx] || 0))),
        color: state.track_muted ? PALETTE.greyDim : PALETTE.trackColors[idx % PALETTE.trackColors.length],
      };
    })
  );

  function describeArc(cx: number, cy: number, r: number, startAngle: number, endAngle: number) {
    const start = polarToCartesian(cx, cy, r, startAngle);
    const end = polarToCartesian(cx, cy, r, endAngle);
    const largeArcFlag = endAngle - startAngle <= 180 ? '0' : '1';
    return ['M', start.x, start.y, 'A', r, r, 0, largeArcFlag, 1, end.x, end.y].join(' ');
  }

  // 135 deg is Bottom-Left (7:30), 270 deg is Top (12:00), 405 deg is Bottom-Right (4:30)
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

  // Upper screen button labels (Buttons 1-4 for left, 5-8 for right)
  let buttonLegends = $derived(
    screen === 'left'
      ? ['PARAM 1-4', 'PAGE <', 'PAGE >', 'DEVICE']
      : ['PARAM 5-8', 'PREV DEV', 'NEXT DEV', 'PAGES']
  );
</script>

<div class="device-view" class:muted={state.track_muted}>
  <!-- Upper Screen Button Legends (Top Buttons 1-4 / 5-8) -->
  <div class="button-legends">
    {#each buttonLegends as legend}
      <div class="button-legend-item">
        <span>{legend}</span>
      </div>
    {/each}
  </div>

  <!-- Page Header -->
  <header class="header">
    <div class="device-badge">DEVICE</div>
    <div class="device-title">
      {#if state.track_muted}<span class="mute-tag">[MUTED]</span>{/if}
      {state.device.toUpperCase() || 'NO DEVICE'}
    </div>
    <div class="screen-indicator">{screen === 'left' ? 'MACROS 1 – 4' : 'MACROS 5 – 8'}</div>
  </header>

  <!-- 4 Encoders -->
  <div class="knobs-row">
    {#each params as param (param.idx)}
      {@const valAngle = ARC_START + param.value * ARC_RANGE}
      {@const modAngle = ARC_START + param.modulated * ARC_RANGE}
      {@const modPos = polarToCartesian(50, 50, 34, modAngle)}
      <div class="knob-container">
        <svg class="knob-svg" viewBox="0 0 100 100">
          <!-- Background track arc -->
          <path
            d={describeArc(50, 50, 34, ARC_START, ARC_END)}
            fill="none"
            stroke="#26262d"
            stroke-width="7"
            stroke-linecap="round"
          />

          <!-- Value arc -->
          {#if param.value > 0.005}
            <path
              d={describeArc(50, 50, 34, ARC_START, valAngle)}
              fill="none"
              stroke={param.color}
              stroke-width="7"
              stroke-linecap="round"
            />
          {/if}

          <!-- Modulated indicator tick -->
          <circle
            cx={modPos.x}
            cy={modPos.y}
            r="3.5"
            fill="#ffffff"
            stroke="#1a1a1a"
            stroke-width="1.5"
          />

          <!-- Center readout -->
          <text x="50" y="52" class="knob-readout" text-anchor="middle" dominant-baseline="middle">
            {param.display}
          </text>
        </svg>

        <div class="knob-label" title={param.name}>
          {param.name}
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .device-view {
    width: 480px;
    height: 272px;
    background: #141416;
    color: #e5e5e7;
    display: flex;
    flex-direction: column;
    user-select: none;
    box-sizing: border-box;
  }

  .button-legends {
    height: 20px;
    background: #0d0d10;
    border-bottom: 1px solid #1f1f26;
    display: flex;
    width: 100%;
  }

  .button-legend-item {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    border-right: 1px solid #1a1a22;
    font-size: 9px;
    font-weight: 700;
    color: #6c6c78;
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }

  .button-legend-item:last-child {
    border-right: none;
  }

  .device-view.muted {
    border: 2px dashed #555555;
  }

  .header {
    height: 32px;
    background: #1a1a1f;
    border-bottom: 1px solid #282830;
    display: flex;
    align-items: center;
    padding: 0 14px;
    gap: 10px;
  }

  .device-badge {
    background: #ff8c1a;
    color: #000;
    font-size: 9px;
    font-weight: 800;
    padding: 2px 6px;
    border-radius: 3px;
    letter-spacing: 0.5px;
  }

  .device-title {
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.3px;
    color: #ffffff;
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mute-tag {
    color: #ff4d4d;
    margin-right: 6px;
  }

  .screen-indicator {
    font-size: 10px;
    font-weight: 600;
    color: #777782;
  }

  .knobs-row {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: space-around;
    padding: 8px 6px;
  }

  .knob-container {
    width: 110px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .knob-svg {
    width: 86px;
    height: 86px;
  }

  .knob-readout {
    font-size: 13px;
    font-weight: 700;
    fill: #ffffff;
    font-family: inherit;
  }

  .knob-label {
    margin-top: 8px;
    font-size: 11px;
    font-weight: 600;
    color: #b5b5ba;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 105px;
  }
</style>
