<script lang="ts">
  import { state, type TrackState } from './store.svelte';

  interface Props {
    screen: 'left' | 'right';
  }

  let { screen }: Props = $props();

  let startIndex = $derived(screen === 'left' ? 0 : 4);
  let currentTracks = $derived<TrackState[]>(state.tracks.slice(startIndex, startIndex + 4));

  let bankLabel = $derived(`TRACKS ${state.bankOffset + 1} – ${state.bankOffset + 8}`);
</script>

<div class="mixer-view">
  <!-- Upper Screen Button Legends Strip (Buttons 1-4 / 5-8) -->
  <div class="button-legends">
    {#each currentTracks as track, i}
      {@const globalIdx = state.bankOffset + startIndex + i + 1}
      <div
        class="button-legend-item"
        class:selected={track.selected}
        style={track.selected ? `border-top: 2px solid ${track.color || '#ff8c1a'}; color: #ffffff; background: rgba(255,140,26,0.12);` : ''}
      >
        <span>{track.exists ? `CH ${globalIdx}` : '—'}</span>
      </div>
    {/each}
  </div>

  <!-- Top Navigation & Bank Header -->
  <header class="header">
    <div class="header-left">
      <div class="mixer-badge">MIXER</div>
      <div class="bank-title">
        <span class="scroll-arrow" class:active={state.bankCanScrollBack}>◄</span>
        <span class="bank-name">{bankLabel}</span>
        <span class="scroll-arrow" class:active={state.bankCanScrollForw}>►</span>
      </div>
    </div>
    <div class="screen-indicator">
      {screen === 'left' ? 'CH 1 – 4' : 'CH 5 – 8'}
    </div>
  </header>

  <!-- 4 Channel Strips -->
  <div class="channel-strips">
    {#each currentTracks as track, i (track.index)}
      {@const globalIdx = state.bankOffset + startIndex + i + 1}
      {@const volPct = Math.max(0, Math.min(100, (track.volume || 0) * 100))}
      {@const meterPct = Math.max(0, Math.min(100, (track.meter || 0) * 100))}
      <div
        class="channel-strip"
        class:selected={track.selected}
        class:muted={track.muted}
        style="--track-color: {track.color || '#ff8c1a'};"
      >
        <!-- Track Color Header -->
        <div class="strip-header" style="background-color: {track.color || '#ff8c1a'};">
          <span class="track-number">{globalIdx}</span>
          <span class="track-name" title={track.name}>{track.name}</span>
        </div>

        <!-- Meter and Fader Area -->
        <div class="meter-fader-area">
          <!-- Pan Indicator Knob / Readout -->
          <div class="pan-box">
            <span class="pan-label">PAN</span>
            <span class="pan-value">{track.panDisplay || 'C'}</span>
          </div>

          <!-- Vertical Fader & VU Meter -->
          <div class="fader-meter-track">
            <!-- Volume Fader -->
            <div class="fader-lane">
              <div class="fader-groove"></div>
              <div class="fader-fill" style="height: {volPct}%;"></div>
              <div class="fader-cap" style="bottom: {volPct}%;">
                <div class="fader-cap-notch"></div>
              </div>
            </div>

            <!-- Peak VU Meter Bar -->
            <div class="meter-lane">
              <div class="meter-bg">
                <div class="meter-fill" style="height: {meterPct}%;"></div>
              </div>
            </div>
          </div>

          <!-- Volume dB Readout -->
          <div class="volume-display">
            {track.volumeDisplay || `${Math.round(volPct)}%`}
          </div>
        </div>

        <!-- Status Badges: Mute, Solo, Arm -->
        <div class="strip-footer">
          <div class="badge mute-badge" class:active={track.muted}>M</div>
          <div class="badge solo-badge" class:active={track.solo}>S</div>
          <div class="badge arm-badge" class:active={track.arm}>●</div>
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .mixer-view {
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
    color: #686874;
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }

  .button-legend-item:last-child {
    border-right: none;
  }

  .header {
    height: 28px;
    background: #18181d;
    border-bottom: 1px solid #282830;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .mixer-badge {
    background: #00bcd4;
    color: #000;
    font-size: 9px;
    font-weight: 800;
    padding: 2px 5px;
    border-radius: 3px;
    letter-spacing: 0.5px;
  }

  .bank-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 700;
    color: #ffffff;
    letter-spacing: 0.4px;
  }

  .scroll-arrow {
    font-size: 10px;
    color: #44444a;
  }

  .scroll-arrow.active {
    color: #00bcd4;
  }

  .screen-indicator {
    font-size: 11px;
    font-weight: 600;
    color: #888892;
  }

  .channel-strips {
    flex: 1;
    display: flex;
    gap: 6px;
    padding: 6px 8px;
    box-sizing: border-box;
  }

  .channel-strip {
    flex: 1;
    background: #1b1b1f;
    border-radius: 4px;
    display: flex;
    flex-direction: column;
    border: 1px solid #282830;
    box-sizing: border-box;
    overflow: hidden;
    position: relative;
    transition: border-color 0.15s ease;
  }

  .channel-strip.selected {
    border: 2px solid var(--track-color, #ff8c1a);
    box-shadow: 0 0 8px rgba(255, 140, 26, 0.25);
  }

  .channel-strip.muted {
    opacity: 0.65;
  }

  .strip-header {
    height: 24px;
    display: flex;
    align-items: center;
    padding: 0 6px;
    gap: 6px;
    color: #000000;
  }

  .track-number {
    font-size: 11px;
    font-weight: 900;
    background: rgba(0, 0, 0, 0.25);
    color: #ffffff;
    padding: 1px 4px;
    border-radius: 2px;
  }

  .track-name {
    font-size: 11px;
    font-weight: 800;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: #000000;
    flex: 1;
  }

  .meter-fader-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 4px 6px 2px 6px;
    gap: 4px;
  }

  .pan-box {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: #141416;
    padding: 2px 5px;
    border-radius: 2px;
    font-size: 10px;
    font-weight: 600;
  }

  .pan-label {
    color: #777780;
  }

  .pan-value {
    color: #e5e5e7;
    font-weight: 700;
  }

  .fader-meter-track {
    flex: 1;
    width: 100%;
    display: flex;
    justify-content: center;
    align-items: stretch;
    gap: 8px;
    position: relative;
    min-height: 80px;
  }

  .fader-lane {
    width: 22px;
    position: relative;
    display: flex;
    justify-content: center;
  }

  .fader-groove {
    position: absolute;
    top: 4px;
    bottom: 4px;
    width: 4px;
    background: #0f0f12;
    border-radius: 2px;
  }

  .fader-fill {
    position: absolute;
    bottom: 4px;
    width: 4px;
    background: var(--track-color, #ff8c1a);
    border-radius: 2px;
    max-height: calc(100% - 8px);
  }

  .fader-cap {
    position: absolute;
    width: 20px;
    height: 12px;
    background: #383842;
    border: 1px solid #5a5a6a;
    border-radius: 2px;
    transform: translateY(50%);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.6);
    pointer-events: none;
  }

  .fader-cap-notch {
    width: 10px;
    height: 2px;
    background: #ffffff;
    border-radius: 1px;
  }

  .meter-lane {
    width: 10px;
    display: flex;
    flex-direction: column;
  }

  .meter-bg {
    flex: 1;
    background: #0d0d10;
    border-radius: 2px;
    overflow: hidden;
    position: relative;
    display: flex;
    flex-direction: column-reverse;
  }

  .meter-fill {
    width: 100%;
    background: linear-gradient(
      to top,
      #2ecc71 0%,
      #2ecc71 65%,
      #f1c40f 80%,
      #e67e22 90%,
      #e74c3c 100%
    );
    border-radius: 2px;
    transition: height 0.05s ease-out;
  }

  .volume-display {
    font-size: 11px;
    font-weight: 700;
    color: #ffffff;
    background: #141417;
    padding: 2px 6px;
    border-radius: 3px;
    width: 100%;
    text-align: center;
    box-sizing: border-box;
  }

  .strip-footer {
    height: 22px;
    background: #141416;
    border-top: 1px solid #26262c;
    display: flex;
    align-items: center;
    justify-content: space-around;
    padding: 0 4px;
  }

  .badge {
    font-size: 10px;
    font-weight: 800;
    color: #484852;
    padding: 1px 4px;
    border-radius: 2px;
  }

  .mute-badge.active {
    background: #ff8c1a;
    color: #000;
  }

  .solo-badge.active {
    background: #ffd11a;
    color: #000;
  }

  .arm-badge.active {
    background: #ff4d4d;
    color: #fff;
  }
</style>
