/**
 * The app-state facade. Screens read signals and call commands here; they
 * never hold a `Controller` or touch a port.
 */

import { signal, type Accessor } from "nsh";

import type { ColorHubPort } from "../application/ports";
import { applyFanProfile, setFanCurve, setFanDuty } from "../application/usecases/fans";
import type {
  ActionResult,
  CurvePoint,
  FanHeaderId,
  FanProfileId,
  Settings,
  SettingsPatch,
  Snapshot,
} from "../domain/model";
import { isPumpHeader } from "../domain/fan";
import { DEFAULT_SETTINGS, mergeSettings } from "../domain/settings";

export interface FanCommands {
  /** Resolves with the message the Rust side chose, verbatim. */
  setDuty(header: FanHeaderId, percent: number): Promise<string>;
  setCurve(header: FanHeaderId, points: CurvePoint[]): Promise<string>;
  applyProfile(profile: FanProfileId): Promise<string>;
}

export interface AppStore {
  snapshot: Accessor<Snapshot | null>;
  settings: Accessor<Settings>;
  version: Accessor<string>;
  loaded: Accessor<boolean>;
  error: Accessor<string | null>;
  fans: FanCommands;
  /** Loads settings, the first snapshot and build facts, then subscribes. */
  start(): Promise<void>;
  refresh(): Promise<void>;
  activity(): Promise<import("../domain/model").ActivityEntry[]>;
  saveSettings(patch: SettingsPatch): Promise<void>;
  dispose(): void;
}

export function createStore(port: ColorHubPort): AppStore {
  const snapshot = signal<Snapshot | null>(null);
  const settings = signal<Settings>(DEFAULT_SETTINGS);
  const version = signal("");
  const loaded = signal(false);
  const error = signal<string | null>(null);
  let stopTelemetry: (() => void) | null = null;

  const applySnapshot = (next: Snapshot): void => snapshot.set(next);
  const applyResult = (result: ActionResult): string => {
    applySnapshot(result.snapshot);
    return result.message;
  };

  const pumpFor = (header: FanHeaderId): boolean => {
    const fan = snapshot()?.fans.find((candidate) => candidate.header === header);
    return fan?.pump ?? isPumpHeader(settings(), header);
  };

  const fans: FanCommands = {
    async setDuty(header, percent) {
      const result = await setFanDuty(port, {
        header,
        percent,
        pump: pumpFor(header),
      });
      return applyResult(result);
    },
    async setCurve(header, points) {
      const result = await setFanCurve(port, {
        header,
        points,
        pump: pumpFor(header),
      });
      return applyResult(result);
    },
    async applyProfile(profile) {
      const headers = snapshot()?.fans.map((fan) => fan.header) ?? [];
      const result = await applyFanProfile(port, { headers, profile });
      return applyResult(result);
    },
  };

  return {
    snapshot,
    settings,
    version,
    loaded,
    error,
    fans,

    async start() {
      try {
        const [nextSettings, nextSnapshot, about] = await Promise.all([
          port.settings(),
          port.snapshot(),
          port.about(),
        ]);
        settings.set(nextSettings);
        snapshot.set(nextSnapshot);
        version.set(about.version);
        stopTelemetry = port.onTelemetry((next) => snapshot.set(next));
      } catch (cause) {
        console.error("session start failed", cause);
        error.set(
          cause instanceof Error
            ? `${cause.message}\n${cause.stack ?? ""}`
            : String(cause),
        );
      } finally {
        loaded.set(true);
      }
    },

    async refresh() {
      const [nextSettings, nextSnapshot] = await Promise.all([
        port.settings(),
        port.reloadState(),
      ]);
      settings.set(nextSettings);
      snapshot.set(nextSnapshot);
    },

    activity: () => port.activity(),

    async saveSettings(patch) {
      settings.set(mergeSettings(settings(), patch));
      const saved = await port.saveSettings(patch);
      settings.set(saved);
    },

    dispose() {
      stopTelemetry?.();
      stopTelemetry = null;
      port.shutdown();
    },
  };
}
