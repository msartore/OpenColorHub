/**
 * Wire model of the Rust service.
 *
 * These are the serde representations `colorctl-core` already writes; the
 * NSH controller forwards them as JSON and `app/infrastructure/native.ts`
 * parses once. Keep the names in step with `crates/colorctl-core/src/model.rs`.
 */

export type FanHeaderId = "sys" | "cpu" | "aux0" | "aux1" | "aux2" | "aux3";
export type FanModeId = "manual" | "smart_fan" | "unknown";
export type BackendMode = "auto" | "hardware" | "simulated";
export type TempUnit = "celsius" | "fahrenheit";
export type FanProfileId = "quiet" | "standard" | "full";
export type BoardConfidence = "verified" | "compatible" | "unknown" | "unsupported";
export type ChannelId = "led" | "rgb1" | "rgb2" | "argb1" | "argb2" | "argb3";

export interface CurvePoint {
  temp_c: number;
  percent: number;
}

export interface FanStatus {
  header: FanHeaderId;
  detected: boolean;
  stalled: boolean;
  label: string;
  description: string;
  bank: number;
  pump: boolean;
  rpm: number;
  pwm: number;
  percent: number;
  mode: FanModeId;
  curve: CurvePoint[] | null;
  curve_percent: number | null;
}

export interface ChannelStatus {
  id: ChannelId;
  label: string;
  name: string;
  description: string;
  leds: number;
  color: [number, number, number];
  hex: string;
  render_hex: string;
  brightness: number;
  enabled: boolean;
}

export interface DeviceInfo {
  name: string;
  detail: string;
  path: string | null;
  available: boolean;
  simulated: boolean;
  error: string | null;
}

export interface Capabilities {
  fan_control: boolean;
  led_control: boolean;
  notes: string[];
}

export interface BoardIdentity {
  vendor: string;
  name: string;
  version: string;
  bios_vendor: string;
  bios_version: string;
  system_vendor: string;
  system_product: string;
}

export interface BoardInfo {
  identity: BoardIdentity | null;
  confidence: BoardConfidence;
  family: string | null;
  series: string | null;
  matched: string | null;
  catalog_size: number;
  source: string;
  notes: string[];
}

export interface Snapshot {
  timestamp_ms: number;
  temperature_c: number | null;
  temperature_source: string | null;
  temperature_register: string | null;
  fans: FanStatus[];
  channels: ChannelStatus[];
  fan_device: DeviceInfo;
  led_device: DeviceInfo;
  capabilities: Capabilities;
  avg_percent: number;
  read_error: string | null;
  board: BoardInfo;
}

export interface ActionResult {
  snapshot: Snapshot;
  message: string;
}

export interface Settings {
  backend: BackendMode;
  poll_interval_ms: number;
  temp_unit: TempUnit;
  smoothing: boolean;
  reapply_led_on_launch: boolean;
  confirm_writes: boolean;
  accent: string;
  state_file: string | null;
  last_screen: string;
  low_duty_warning: number;
  pump_header: FanHeaderId | null;
}

export type SettingsPatch = Partial<Omit<Settings, "pump_header">> & {
  pump_header?: FanHeaderId | "none";
};

export interface ActivityEntry {
  at_ms: number;
  kind: string;
  message: string;
}

export interface ElevationState {
  elevated: boolean;
  can_request: boolean;
  detail: string;
}

export interface AutostartState {
  /** False on platforms with no implementation. */
  supported: boolean;
  /** Whether the OS currently has a logon entry for this user. */
  enabled: boolean;
  /** `logon-task`, `xdg-autostart` or `none`. */
  mechanism: string;
  /** Where the entry lives, shown verbatim. */
  detail: string;
}

/** A preset covers one half of the machine, never both. */
export type SetupKind = "fans" | "lighting";

export interface SetupFanSetting {
  header: FanHeaderId;
  percent: number;
  curve: CurvePoint[] | null;
}

export interface SetupChannelSetting {
  channel: ChannelId;
  color: [number, number, number];
  brightness: number;
  enabled: boolean;
}

export interface Setup {
  id: string;
  name: string;
  kind: SetupKind;
  saved_at_ms: number;
  fans: SetupFanSetting[];
  channels: SetupChannelSetting[];
}

export interface SetupList {
  setups: Setup[];
  warning: string | null;
}

export interface BuildMeta {
  version: string;
  tauri: string | null;
  rustc: string;
  profile: string;
  os: string;
  arch: string;
  family: string;
}

export interface AboutInfo {
  name: string;
  version: string;
  tagline: string;
  license: string;
  build: BuildMeta;
  upstream: { name: string; url: string; author: string; license: string };
  chip: string;
  chip_id: number;
  total_leds: number;
  board: BoardInfo;
  paths: {
    data_dir: string;
    settings: string;
    state: string;
    config_override: string | null;
  };
}
