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
  DownloadedFile,
  DriverFile,
  DriverList,
  DriverProgress,
  ElevationState,
  FanHeaderId,
  FanProfileId,
  PawnioProgress,
  PawnioStatus,
  Settings,
  SettingsPatch,
  SetupKind,
  SetupList,
  Snapshot,
  StoredFile,
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

/** For async members: the native call returns a Promise of the JSON. */
async function callAsync<T>(run: () => Promise<string>): Promise<T> {
  try {
    return parse<T>(await run());
  } catch (error) {
    throw toAppError(error);
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

    drivers: (refresh: boolean) =>
      callAsync<DriverList>(() => controller.driversJson(refresh)),
    downloadDriver: async (
      file: DriverFile,
      onProgress: (progress: DriverProgress) => void,
    ): Promise<DownloadedFile> => {
      try {
        const json = await controller.downloadDriver(JSON.stringify(file), (progress) => {
          try {
            onProgress(parse<DriverProgress>(progress));
          } catch (error) {
            console.error("driver progress could not be read", error);
          }
        });
        return parse<DownloadedFile>(json);
      } catch (error) {
        throw toAppError(error);
      }
    },
    downloadedDrivers: () => call<StoredFile[]>(() => controller.downloadedDriversJson()),
    revealDriver: async (path: string) => {
      try {
        await controller.revealDriver(path);
      } catch (error) {
        throw toAppError(error);
      }
    },

    pawnioStatus: () => call<PawnioStatus>(() => controller.pawnioStatusJson()),
    installPawnio: async (onProgress: (progress: PawnioProgress) => void) => {
      try {
        const json = await controller.installPawnioJson((progress) => {
          try {
            onProgress(parse<PawnioProgress>(progress));
          } catch (error) {
            console.error("PawnIO progress could not be read", error);
          }
        });
        return parse<PawnioStatus>(json);
      } catch (error) {
        throw toAppError(error);
      }
    },

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
