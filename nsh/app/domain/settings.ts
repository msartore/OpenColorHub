/**
 * Preference rules. Mirrors `Settings::default`, `apply_patch` and `sanitize`
 * in `crates/colorctl-core/src/settings.rs`.
 */

import type { BackendMode, FanHeaderId, Settings, SettingsPatch, TempUnit } from "./model";

export const POLL_MIN_MS = 250;
export const POLL_MAX_MS = 10_000;
export const DEFAULT_ACCENT = "#4ade80";
export const BACKEND_MODES: readonly BackendMode[] = ["auto", "hardware", "simulated"];
export const TEMP_UNITS: readonly TempUnit[] = ["celsius", "fahrenheit"];

export const DEFAULT_SETTINGS: Settings = {
  backend: "auto",
  poll_interval_ms: 1000,
  temp_unit: "celsius",
  smoothing: true,
  restore_setup_on_startup: true,
  release_fans_on_exit: false,
  gpu_led_enabled: true,
  reapply_led_on_launch: false,
  confirm_writes: true,
  accent: DEFAULT_ACCENT,
  state_file: null,
  last_screen: "dashboard",
  low_duty_warning: 20,
  pump_header: null,
};

const FAN_HEADER_IDS: readonly FanHeaderId[] = [
  "sys",
  "cpu",
  "aux0",
  "aux1",
  "aux2",
  "aux3",
];

/** "Reset to defaults" is a patch: the state-file override and last screen stay. */
export const SETTINGS_RESET_PATCH: SettingsPatch = {
  backend: DEFAULT_SETTINGS.backend,
  poll_interval_ms: DEFAULT_SETTINGS.poll_interval_ms,
  temp_unit: DEFAULT_SETTINGS.temp_unit,
  smoothing: DEFAULT_SETTINGS.smoothing,
  restore_setup_on_startup: DEFAULT_SETTINGS.restore_setup_on_startup,
  release_fans_on_exit: DEFAULT_SETTINGS.release_fans_on_exit,
  gpu_led_enabled: DEFAULT_SETTINGS.gpu_led_enabled,
  reapply_led_on_launch: DEFAULT_SETTINGS.reapply_led_on_launch,
  confirm_writes: DEFAULT_SETTINGS.confirm_writes,
  accent: DEFAULT_SETTINGS.accent,
  low_duty_warning: DEFAULT_SETTINGS.low_duty_warning,
  pump_header: "none",
};

export function isAccent(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

export function isFanHeader(value: unknown): value is FanHeaderId {
  return typeof value === "string" && FAN_HEADER_IDS.includes(value as FanHeaderId);
}

export function normalizeSettings(settings: Settings): Settings {
  const poll = Number.isFinite(settings.poll_interval_ms)
    ? settings.poll_interval_ms
    : DEFAULT_SETTINGS.poll_interval_ms;
  const lowDuty = Number.isFinite(settings.low_duty_warning)
    ? settings.low_duty_warning
    : DEFAULT_SETTINGS.low_duty_warning;
  return {
    ...settings,
    backend: BACKEND_MODES.includes(settings.backend) ? settings.backend : "auto",
    poll_interval_ms: Math.min(POLL_MAX_MS, Math.max(POLL_MIN_MS, Math.round(poll))),
    temp_unit: TEMP_UNITS.includes(settings.temp_unit) ? settings.temp_unit : "celsius",
    restore_setup_on_startup:
      typeof settings.restore_setup_on_startup === "boolean"
        ? settings.restore_setup_on_startup
        : DEFAULT_SETTINGS.restore_setup_on_startup,
    release_fans_on_exit:
      typeof settings.release_fans_on_exit === "boolean"
        ? settings.release_fans_on_exit
        : DEFAULT_SETTINGS.release_fans_on_exit,
    gpu_led_enabled:
      typeof settings.gpu_led_enabled === "boolean"
        ? settings.gpu_led_enabled
        : DEFAULT_SETTINGS.gpu_led_enabled,
    accent: isAccent(settings.accent) ? settings.accent : DEFAULT_ACCENT,
    low_duty_warning: Math.min(100, Math.max(0, Math.round(lowDuty))),
    last_screen: settings.last_screen.trim() || DEFAULT_SETTINGS.last_screen,
    pump_header: isFanHeader(settings.pump_header) ? settings.pump_header : null,
  };
}

/** `pump_header: "none"` clears the binding; an unknown id leaves it alone. */
export function mergeSettings(current: Settings, patch: SettingsPatch): Settings {
  const next: Settings = { ...current };
  if (patch.backend !== undefined) next.backend = patch.backend;
  if (patch.poll_interval_ms !== undefined) next.poll_interval_ms = patch.poll_interval_ms;
  if (patch.temp_unit !== undefined) next.temp_unit = patch.temp_unit;
  if (patch.smoothing !== undefined) next.smoothing = patch.smoothing;
  if (patch.restore_setup_on_startup !== undefined) {
    next.restore_setup_on_startup = patch.restore_setup_on_startup;
  }
  if (patch.release_fans_on_exit !== undefined) {
    next.release_fans_on_exit = patch.release_fans_on_exit;
  }
  if (patch.gpu_led_enabled !== undefined) next.gpu_led_enabled = patch.gpu_led_enabled;
  if (patch.reapply_led_on_launch !== undefined) {
    next.reapply_led_on_launch = patch.reapply_led_on_launch;
  }
  if (patch.confirm_writes !== undefined) next.confirm_writes = patch.confirm_writes;
  if (patch.accent !== undefined) next.accent = patch.accent;
  if (patch.state_file !== undefined) next.state_file = patch.state_file;
  if (patch.last_screen !== undefined) next.last_screen = patch.last_screen;
  if (patch.low_duty_warning !== undefined) {
    next.low_duty_warning = patch.low_duty_warning;
  }
  if (patch.pump_header !== undefined) {
    if (patch.pump_header === "none") next.pump_header = null;
    else if (isFanHeader(patch.pump_header)) next.pump_header = patch.pump_header;
  }
  return normalizeSettings(next);
}
