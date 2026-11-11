/**
 * The Tauri IPC adapter, ported to NSH's generated native module.
 *
 * This is the only file that names the Rust class. It parses each JSON answer
 * once and hands typed values inward; a rejection carries `AppError`.
 */

import { Controller } from "opencolorhub/native";

import type { ColorHubPort } from "../application/ports";
import { toAppError } from "../application/errors";
import type {
  AboutInfo,
  ActionResult,
  ActivityEntry,
  AutostartState,
  ChannelId,
  CurvePoint,
  ElevationState,
  FanHeaderId,
  FanProfileId,
  Settings,
  SettingsPatch,
  SetupKind,
  SetupList,
  Snapshot,
} from "../domain/model";
import type { Rgb } from "../domain/lighting";

function parse<T>(json: string): T {
  return JSON.parse(json) as T;
}

/** Runs a synchronous native call, turning both parse and Rust errors into one. */
function call<T>(run: () => string): Promise<T> {
  try {
    return Promise.resolve(parse<T>(run()));
  } catch (error) {
    return Promise.reject(toAppError(error));
  }
}

export function createNativePort(controller: Controller): ColorHubPort {
  return {
    snapshot: () => call<Snapshot>(() => controller.snapshotJson()),
    settings: () => call<Settings>(() => controller.settingsJson()),
    saveSettings: (patch: SettingsPatch) =>
      call<Settings>(() => controller.saveSettingsJson(JSON.stringify(patch))),
    about: () => call<AboutInfo>(() => controller.aboutJson()),
    activity: () => call<ActivityEntry[]>(() => controller.activityJson()),
    reloadState: () => call<Snapshot>(() => controller.reloadStateJson()),
    elevation: () => call<ElevationState>(() => controller.elevationJson()),
    requestElevation: () => call<string>(() => controller.requestElevation()),
    autostart: () => call<AutostartState>(() => controller.autostartJson()),
    setAutostart: (enabled: boolean) =>
      call<AutostartState>(() => controller.setAutostart(enabled)),

    listSetups: (kind: SetupKind) =>
      call<SetupList>(() => controller.listSetups(kind)),
    saveSetup: (kind: SetupKind, name: string, id?: string) =>
      call<SetupList>(() => controller.saveSetup(kind, name, id ?? null)),
    applySetup: (id: string) =>
      call<ActionResult>(() => controller.applySetup(id)),
    deleteSetup: (kind: SetupKind, id: string) =>
      call<SetupList>(() => controller.deleteSetup(kind, id)),

    setFanPercent: (header: FanHeaderId, percent: number) =>
      call<ActionResult>(() => controller.setFanPercent(header, percent)),
    setFanCurve: (header: FanHeaderId, points: CurvePoint[]) =>
      call<ActionResult>(() => controller.setFanCurve(header, JSON.stringify(points))),
    applyProfile: (headers: FanHeaderId[], profile: FanProfileId) =>
      call<ActionResult>(() => controller.applyProfile(JSON.stringify(headers), profile)),

    setChannel: (channel: ChannelId, color: Rgb, brightness: number, enabled: boolean) =>
      call<ActionResult>(() =>
        controller.setChannel(
          channel,
          JSON.stringify([color[0], color[1], color[2]]),
          Math.round(brightness),
          enabled,
        ),
      ),
    setAllChannels: (color: Rgb, brightness: number, enabled: boolean) =>
      call<ActionResult>(() =>
        controller.setAllChannels(
          JSON.stringify([color[0], color[1], color[2]]),
          Math.round(brightness),
          enabled,
        ),
      ),
    setBrightness: (percent: number) =>
      call<ActionResult>(() => controller.setBrightness(Math.round(percent))),
    turnOffChannel: (channel: ChannelId) =>
      call<ActionResult>(() => controller.turnOffChannel(channel)),
    applyLed: () => call<ActionResult>(() => controller.applyLed()),

    onTelemetry: (listener) => {
      void controller.startTelemetry((json) => {
        try {
          listener(parse<Snapshot>(json));
        } catch (error) {
          console.error("telemetry frame could not be read", error);
        }
      });
      return () => controller.stopTelemetry();
    },

    shutdown: () => controller.shutdown(),
  };
}
