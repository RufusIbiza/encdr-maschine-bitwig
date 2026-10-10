export interface TrackState {
  index: number;
  name: string;
  color: string;
  volume: number; // 0.0 - 1.0
  volumeDisplay: string;
  pan: number; // 0.0 - 1.0 (0.5 = center)
  panDisplay: string;
  meter: number; // 0.0 - 1.0 peak
  muted: boolean;
  solo: boolean;
  arm: boolean;
  exists: boolean;
  activated?: boolean;
  selected: boolean;
  isGroup?: boolean;
  isGroupExpanded?: boolean;
  /** Nesting depth in the track hierarchy (0 = top level). */
  depth?: number;
  hasParent?: boolean;
  parentColor?: string;
  parentName?: string;
  hasGrandparent?: boolean;
  grandparentColor?: string;
  grandparentName?: string;
}

export interface SendState {
  index: number;
  name: string;
  value: number;
  display: string;
}

export interface AppState {
  page: 'device' | 'mixer' | 'channel';
  // Phase 1 Device State
  device: string;
  names: string[];
  displays: string[];
  values: number[];
  modulated: number[];
  track_muted: boolean;
  
  // Phase 2 Mixer & Track Bank State
  tracks: TrackState[];
  /** Every (visible) track in the project, for the channel strip; hierarchy fields only. */
  strip: TrackState[];
  stripSelectedIndex: number;
  bankOffset: number;
  bankCanScrollBack: boolean;
  bankCanScrollForw: boolean;
  
  // Phase 2 Selected Track / Channel State
  selectedTrackIndex: number;
  selectedTrackName: string;
  selectedTrackColor: string;
  selectedTrackVolume: number;
  selectedTrackVolumeDisplay: string;
  selectedTrackPan: number;
  selectedTrackPanDisplay: string;
  selectedTrackMeter: number;
  selectedTrackMeterL: number;
  selectedTrackMeterR: number;
  selectedTrackMaxPeak: number;
  selectedTrackMuted: boolean;
  selectedTrackSolo: boolean;
  selectedTrackArm: boolean;
  channelIsGroup: boolean;
  channelIsGroupExpanded: boolean;
  channelIsMaster: boolean;
  channelMonitorMode: string;
  masterName: string;
  /** Finger on the 4D encoder. */
  navTouched: boolean;
  /** Increments on every 4D-encoder navigation. */
  navSeq: number;
  masterColor: string;
  sends: SendState[];
  sendsPage: number;
  sendsTotal: number;
}

function createDefaultTracks(): TrackState[] {
  return Array.from({ length: 16 }, (_, i) => ({
    index: i,
    name: `Track ${i + 1}`,
    color: '#ff8c1a',
    volume: 0.8,
    volumeDisplay: '0.0 dB',
    pan: 0.5,
    panDisplay: 'C',
    meter: 0.0,
    muted: false,
    solo: false,
    arm: false,
    exists: false,
    selected: i === 0,
    isGroup: false,
    isGroupExpanded: false,
    depth: 0,
    hasParent: false,
    parentColor: '',
    parentName: '',
    hasGrandparent: false,
    grandparentColor: '',
    grandparentName: '',
  }));
}

function createDefaultSends(): SendState[] {
  return Array.from({ length: 12 }, (_, i) => ({
    index: i,
    name: `Send ${String.fromCharCode(65 + i)}`,
    value: 0.0,
    display: '-inf dB',
  }));
}

function toTrackState(t: any, idx: number): TrackState {
  return {
    index: t.index ?? idx,
    name: t.name ?? `Track ${idx + 1}`,
    color: t.color ?? '#ff8c1a',
    volume: t.volume ?? 0.8,
    volumeDisplay: t.volume_display ?? t.volumeDisplay ?? '0.0 dB',
    pan: t.pan ?? 0.5,
    panDisplay: t.pan_display ?? t.panDisplay ?? 'C',
    meter: t.meter ?? 0.0,
    muted: !!t.muted,
    solo: !!t.solo,
    arm: !!t.arm,
    exists: !!t.exists,
    activated: t.activated !== false,
    selected: !!t.selected,
    isGroup: !!(t.is_group ?? t.isGroup),
    isGroupExpanded: !!(t.is_group_expanded ?? t.isGroupExpanded),
    depth: Number(t.depth ?? 0) || 0,
    hasParent: !!(t.has_parent ?? t.hasParent),
    parentColor: t.parent_color ?? t.parentColor ?? '',
    parentName: t.parent_name ?? t.parentName ?? '',
    hasGrandparent: !!(t.has_grandparent ?? t.hasGrandparent),
    grandparentColor: t.grandparent_color ?? t.grandparentColor ?? '',
    grandparentName: t.grandparent_name ?? t.grandparentName ?? '',
  };
}

export const state = $state<AppState>({
  page: 'mixer',
  device: 'No Device',
  names: ['', '', '', '', '', '', '', ''],
  displays: ['', '', '', '', '', '', '', ''],
  values: [0, 0, 0, 0, 0, 0, 0, 0],
  modulated: [0, 0, 0, 0, 0, 0, 0, 0],
  track_muted: false,

  tracks: createDefaultTracks(),
  strip: [],
  stripSelectedIndex: -1,
  bankOffset: 0,
  bankCanScrollBack: false,
  bankCanScrollForw: false,

  selectedTrackIndex: 0,
  navTouched: false,
  navSeq: 0,
  selectedTrackName: 'No Track',
  selectedTrackColor: '#ff8c1a',
  selectedTrackVolume: 0.8,
  selectedTrackVolumeDisplay: '0.0 dB',
  selectedTrackPan: 0.5,
  selectedTrackPanDisplay: 'C',
  selectedTrackMeter: 0.0,
  selectedTrackMeterL: 0.0,
  selectedTrackMeterR: 0.0,
  selectedTrackMaxPeak: 0.0,
  selectedTrackMuted: false,
  selectedTrackSolo: false,
  selectedTrackArm: false,
  channelIsGroup: false,
  channelIsGroupExpanded: false,
  channelIsMaster: false,
  channelMonitorMode: 'AUTO',
  masterName: 'Master',
  masterColor: '#4a90e2',
  sends: createDefaultSends(),
  sendsPage: 0,
  sendsTotal: 6,
});

declare global {
  interface Window {
    encdr?: {
      onMessage?: (channel: string, data: any) => void;
      requestCapture?: () => void;
      _dirty?: boolean;
      _rafId?: number | null;
    };
    ipc?: {
      postMessage: (msg: string) => void;
    };
  }
}

export function initEncdrBridge() {
  if (typeof window === 'undefined') return;

  const existingHandler = window.encdr?.onMessage;
  window.encdr = window.encdr || {};

  window.encdr.onMessage = (channel: string, data: any) => {
    if (channel === 'state' && data && typeof data === 'object') {
      if (data.page !== undefined) state.page = data.page;
      if (data.device !== undefined) state.device = data.device;
      if (data.names !== undefined) state.names = data.names;
      if (data.displays !== undefined) state.displays = data.displays;
      if (data.values !== undefined) state.values = data.values;
      if (data.modulated !== undefined) state.modulated = data.modulated;
      if (data.track_muted !== undefined) state.track_muted = data.track_muted;

      if (data.bankOffset !== undefined) state.bankOffset = data.bankOffset;
      if (data.bankCanScrollBack !== undefined) state.bankCanScrollBack = data.bankCanScrollBack;
      if (data.bankCanScrollForw !== undefined) state.bankCanScrollForw = data.bankCanScrollForw;

      if (Array.isArray(data.strip)) state.strip = data.strip.map(toTrackState);
      if (data.stripSelectedIndex !== undefined) state.stripSelectedIndex = data.stripSelectedIndex;

      if (Array.isArray(data.tracks)) {
        state.tracks = data.tracks.map(toTrackState);
      }

      if (data.selectedTrackIndex !== undefined) state.selectedTrackIndex = data.selectedTrackIndex;
      if (data.selectedTrackName !== undefined) state.selectedTrackName = data.selectedTrackName;
      if (data.selectedTrackColor !== undefined) state.selectedTrackColor = data.selectedTrackColor;
      if (data.selectedTrackVolume !== undefined) state.selectedTrackVolume = data.selectedTrackVolume;
      if (data.selectedTrackVolumeDisplay !== undefined) state.selectedTrackVolumeDisplay = data.selectedTrackVolumeDisplay;
      if (data.selectedTrackPan !== undefined) state.selectedTrackPan = data.selectedTrackPan;
      if (data.selectedTrackPanDisplay !== undefined) state.selectedTrackPanDisplay = data.selectedTrackPanDisplay;
      if (data.selectedTrackMeter !== undefined) state.selectedTrackMeter = data.selectedTrackMeter;
      if (data.selectedTrackMeterL !== undefined) state.selectedTrackMeterL = data.selectedTrackMeterL;
      if (data.selectedTrackMeterR !== undefined) state.selectedTrackMeterR = data.selectedTrackMeterR;
      if (data.selectedTrackMaxPeak !== undefined) state.selectedTrackMaxPeak = data.selectedTrackMaxPeak;
      if (data.selectedTrackMuted !== undefined) state.selectedTrackMuted = data.selectedTrackMuted;
      if (data.selectedTrackSolo !== undefined) state.selectedTrackSolo = data.selectedTrackSolo;
      if (data.selectedTrackArm !== undefined) state.selectedTrackArm = data.selectedTrackArm;
      if (data.channelIsGroup !== undefined) state.channelIsGroup = data.channelIsGroup;
      if (data.channelIsGroupExpanded !== undefined) state.channelIsGroupExpanded = data.channelIsGroupExpanded;
      if (data.channelIsMaster !== undefined) state.channelIsMaster = data.channelIsMaster;
      if (data.channelMonitorMode !== undefined) state.channelMonitorMode = data.channelMonitorMode;
      if (data.masterName !== undefined) state.masterName = data.masterName;
      if (data.masterColor !== undefined) state.masterColor = data.masterColor;
      if (data.navTouched !== undefined) state.navTouched = !!data.navTouched;
      if (data.navSeq !== undefined) state.navSeq = data.navSeq;

      if (data.sendsPage !== undefined) state.sendsPage = data.sendsPage;
      if (data.sendsTotal !== undefined) state.sendsTotal = data.sendsTotal;

      if (Array.isArray(data.sends)) {
        for (let i = 0; i < Math.min(data.sends.length, 12); i++) {
          Object.assign(state.sends[i], data.sends[i]);
        }
      }
    }

    if (existingHandler) {
      existingHandler(channel, data);
    }
  };
}
