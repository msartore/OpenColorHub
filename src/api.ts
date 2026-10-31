/** IPC bridge: real Tauri commands, or the browser demo backend. */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { inTauri } from "./dom";
import { createDemoBackend } from "./mock";
import type {
  AboutInfo,
  AutostartState,
  ActionResult,
  ChannelId,
  CurvePoint,
  ActionResult as Result,
  ElevationState,
  FanHeaderId,
  FanProfileId,
  Settings,
  SettingsPatch,
  DriverFile,
  DriverList,
  DownloadedFile,
  DriverProgress,
  SetupList,
  PawnioStatus,
  PawnioProgress,
  Snapshot,
  ActivityEntry,
} from "./types";

const demo = inTauri() ? null : createDemoBackend();

/** True when the UI is running in a plain browser (design/demo mode). */
export const isDemo = demo !== null;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (demo) return demo.invoke<T>(cmd, args ?? {});
  return tauriInvoke<T>(cmd, args);
}

export const api = {
  snapshot: () => call<Snapshot>("get_snapshot"),
  about: () => call<AboutInfo>("get_about"),
  activity: () => call<ActivityEntry[]>("get_activity"),
  settings: () => call<Settings>("get_settings"),
  saveSettings: (patch: SettingsPatch) => call<Settings>("save_settings", { patch }),

  elevation: () => call<ElevationState>("get_elevation"),
  /** Resolves to "launched" or "declined"; on "launched" this process exits. */
  requestElevation: () => call<string>("request_elevation"),

  autostart: () => call<AutostartState>("get_autostart"),
  setAutostart: (enabled: boolean) => call<AutostartState>("set_autostart", { enabled }),

  setFanPercent: (header: FanHeaderId, percent: number) =>
    call<Result>("set_fan_percent", { header, percent }),
  setFanPwm: (header: FanHeaderId, pwm: number) => call<Result>("set_fan_pwm", { header, pwm }),
  setFanCurve: (header: FanHeaderId, points: CurvePoint[]) =>
    call<Result>("set_fan_curve", { header, points }),
  applyProfile: (headers: FanHeaderId[], profile: FanProfileId) =>
    call<Result>("apply_profile", { headers, profile }),

  setChannel: (
    channel: ChannelId,
    color: [number, number, number],
    brightness: number,
    enabled: boolean,
  ) => call<Result>("set_channel", { channel, color, brightness, enabled }),
  setAllChannels: (color: [number, number, number], brightness: number, enabled: boolean) =>
    call<ActionResult>("set_all_channels", { color, brightness, enabled }),
  setBrightness: (percent: number) => call<ActionResult>("set_brightness", { percent }),
  turnOffChannel: (channel: ChannelId) => call<ActionResult>("turn_off_channel", { channel }),
  applyLed: () => call<ActionResult>("apply_led"),

  openExternal: (url: string) => call<void>("open_external", { url }),

  /** Vendor downloads for the detected board. Hits the network; may take a second. */
  drivers: (refresh = false) => call<DriverList>("get_drivers", { refresh }),
  downloadDriver: (file: DriverFile) =>
    call<DownloadedFile>("download_driver", { file }),
  revealDriver: (path: string) => call<void>("reveal_driver", { path }),

  pawnioStatus: () => call<PawnioStatus>("pawnio_status"),
  installPawnio: () => call<PawnioStatus>("install_pawnio"),

  setups: () => call<SetupList>("list_setups"),
  saveSetup: (name: string, id?: string) =>
    call<SetupList>("save_setup", { name, id: id ?? null }),
  applySetup: (id: string) => call<Result>("apply_setup", { id }),
  deleteSetup: (id: string) => call<SetupList>("delete_setup", { id }),
};

/** Subscribes to telemetry ticks; returns an unsubscribe function. */
/** Resolves when the backend finishes probing the hardware. */
export async function onBackendReady(cb: () => void): Promise<() => void> {
  if (demo) return () => {};
  return listen("backend://ready", () => cb());
}

/** Subscribes to the PawnIO installer download. */
export async function onPawnioProgress(
  cb: (progress: PawnioProgress) => void,
): Promise<() => void> {
  if (demo) return () => {};
  return listen<PawnioProgress>("pawnio://progress", (event) => cb(event.payload));
}

/** Subscribes to download progress; resolves once the listener is installed. */
export async function onDriverProgress(
  cb: (progress: DriverProgress) => void,
): Promise<() => void> {
  if (demo) return () => {};
  return listen<DriverProgress>("drivers://progress", (event) => cb(event.payload));
}

export async function onTelemetry(cb: (snapshot: Snapshot) => void): Promise<() => void> {
  if (demo) return demo.subscribe(cb);
  const unlisten = await listen<Snapshot>("telemetry", (event) => cb(event.payload));
  return unlisten;
}
