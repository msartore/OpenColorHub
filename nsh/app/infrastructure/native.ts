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
  CurvePoint,
  FanHeaderId,
  FanProfileId,
  Settings,
  SettingsPatch,
  Snapshot,
} from "../domain/model";

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

    setFanPercent: (header: FanHeaderId, percent: number) =>
      call<ActionResult>(() => controller.setFanPercent(header, percent)),
    setFanCurve: (header: FanHeaderId, points: CurvePoint[]) =>
      call<ActionResult>(() => controller.setFanCurve(header, JSON.stringify(points))),
    applyProfile: (headers: FanHeaderId[], profile: FanProfileId) =>
      call<ActionResult>(() => controller.applyProfile(JSON.stringify(headers), profile)),

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
