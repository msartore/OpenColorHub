/**
 * Ports the use cases need, in domain vocabulary.
 *
 * They are slices of one capability — the desktop backend — so the composition
 * root wires one adapter and each use case receives only the methods it calls.
 * A fake for a use-case test is a small object, not a mock of the whole API.
 */

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

export type Unlisten = () => void;

export interface TelemetryPort {
  snapshot(): Promise<Snapshot>;
  activity(): Promise<ActivityEntry[]>;
  reloadState(): Promise<Snapshot>;
  onTelemetry(listener: (snapshot: Snapshot) => void): Unlisten;
}

export interface SettingsPort {
  settings(): Promise<Settings>;
  saveSettings(patch: SettingsPatch): Promise<Settings>;
}

export interface FanPort {
  setFanPercent(header: FanHeaderId, percent: number): Promise<ActionResult>;
  setFanCurve(header: FanHeaderId, points: CurvePoint[]): Promise<ActionResult>;
  applyProfile(headers: FanHeaderId[], profile: FanProfileId): Promise<ActionResult>;
}

export interface SystemPort {
  about(): Promise<AboutInfo>;
  shutdown(): void;
}

export interface ColorHubPort
  extends TelemetryPort,
    SettingsPort,
    FanPort,
    SystemPort {}
