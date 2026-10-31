/**
 * Wire types. These mirror the serde representations in
 * `crates/colorctl-core/src/model.rs` — keep both sides in sync.
 */

export type FanHeaderId = "cpu" | "cha_fan1" | "cha_fan2" | "cha_fan3" | "pump";
export type ChannelId = "led" | "rgb1" | "rgb2" | "argb1" | "argb2" | "argb3";
export type FanModeId = "manual" | "smart_fan" | "unknown";
export type BackendMode = "auto" | "hardware" | "simulated";
export type TempUnit = "celsius" | "fahrenheit";
export type FanProfileId = "quiet" | "standard" | "full";
export type BoardConfidence = "verified" | "compatible" | "unknown" | "unsupported";
export type ScreenId =
  | "dashboard"
  | "fans"
  | "leds"
  | "drivers"
  | "setups"
  | "settings"
  | "about";

/** Privilege state, as reported by `get_elevation`. */
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

export interface ElevationState {
  elevated: boolean;
  can_request: boolean;
  detail: string;
}

export interface CurvePoint {
  temp_c: number;
  percent: number;
}

/** The chip stores exactly four points; the wire type is a plain array. */
export type FanCurve = CurvePoint[];

export interface FanStatus {
  header: FanHeaderId;
  label: string;
  description: string;
  bank: number;
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
  /** "dmi", "registry", "demo" or "none". */
  source: string;
  notes: string[];
}

export interface Snapshot {
  timestamp_ms: number;
  temperature_c: number | null;
  fans: FanStatus[];
  channels: ChannelStatus[];
  fan_device: DeviceInfo;
  led_device: DeviceInfo;
  capabilities: Capabilities;
  avg_percent: number;
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
}

export type SettingsPatch = Partial<Settings>;

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
  vid: number;
  pid: number;
  total_leds: number;
  board: BoardInfo;
  boards: string[];
  headers: { id: string; label: string; bank: number; description: string }[];
  channels: { id: string; label: string; leds: number; range: string; description: string }[];
  capabilities: string[];
  paths: {
    data_dir: string;
    settings: string;
    state: string;
    config_override: string | null;
  };
  links: { label: string; url: string; kind: string }[];
}

export interface ActivityEntry {
  at_ms: number;
  kind: string;
  message: string;
}

/** What a vendor download is, as classified by the core. */
export type DriverKind = "bios" | "driver" | "utility" | "manual" | "other";

export interface DriverFile {
  title: string;
  kind: DriverKind;
  url: string;
  /** Server-provided name, already sanitised for the local filesystem. */
  file_name: string;
  /** `YYYY-MM-DD`, empty when the vendor published no date. */
  published: string;
}

export interface DriverRow extends DriverFile {
  /** True when this is firmware newer than the installed BIOS. */
  newer: boolean;
}

export interface DriverList {
  board: string;
  product_id: number;
  /** BIOS version reported by SMBIOS. */
  installed_bios: string;
  files: DriverRow[];
  /** When the vendor was last asked, in ms since the epoch. */
  fetched_at_ms: number;
  /** True when this came from the local copy rather than the network. */
  cached: boolean;
  /** Set when a refresh failed but a cached copy could still be served. */
  warning: string | null;
}

export interface DownloadedFile {
  path: string;
  bytes: number;
  sha256: string;
}

export interface DriverProgress {
  file: string;
  received: number;
  total: number | null;
}

/** One saved combination of fan and lighting settings. */
export interface FanSetting {
  header: FanHeaderId;
  percent: number;
  curve: CurvePoint[] | null;
}

export interface ChannelSetting {
  channel: ChannelId;
  color: [number, number, number];
  brightness: number;
  enabled: boolean;
}

export interface Setup {
  id: string;
  name: string;
  saved_at_ms: number;
  fans: FanSetting[];
  channels: ChannelSetting[];
}

export interface SetupList {
  setups: Setup[];
  warning: string | null;
}

/** Whether fan control can reach PawnIO's driver. */
export type PawnioStatus =
  | { state: "ready"; version: string }
  | { state: "not_installed"; detail: string }
  | { state: "denied"; detail: string }
  | { state: "failed"; detail: string }
  | { state: "unsupported" };

export interface PawnioProgress {
  received: number;
  total: number | null;
}
