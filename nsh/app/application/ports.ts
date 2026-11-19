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
  GpuZoneState,
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

export interface LightingPort {
  setChannel(
    channel: ChannelId,
    color: Rgb,
    brightness: number,
    enabled: boolean,
  ): Promise<ActionResult>;
  setAllChannels(color: Rgb, brightness: number, enabled: boolean): Promise<ActionResult>;
  setBrightness(percent: number): Promise<ActionResult>;
  turnOffChannel(channel: ChannelId): Promise<ActionResult>;
  applyLed(): Promise<ActionResult>;
}

export interface GpuPort {
  setGpuZone(
    device: string,
    zone: string,
    state: GpuZoneState,
    apply: boolean,
  ): Promise<ActionResult>;
  applyGpu(): Promise<ActionResult>;
  syncGpuFromBoard(): Promise<ActionResult>;
}

export interface SetupPort {
  listSetups(kind: SetupKind): Promise<SetupList>;
  saveSetup(kind: SetupKind, name: string, id?: string): Promise<SetupList>;
  applySetup(id: string): Promise<ActionResult>;
  deleteSetup(kind: SetupKind, id: string): Promise<SetupList>;
}

export interface DriverPort {
  drivers(refresh: boolean): Promise<DriverList>;
  downloadDriver(
    file: DriverFile,
    onProgress: (progress: DriverProgress) => void,
  ): Promise<DownloadedFile>;
  downloadedDrivers(): Promise<StoredFile[]>;
  revealDriver(path: string): Promise<void>;
}

export interface PawnioPort {
  pawnioStatus(): Promise<PawnioStatus>;
  installPawnio(onProgress: (progress: PawnioProgress) => void): Promise<PawnioStatus>;
}

export interface SystemPort {
  about(): Promise<AboutInfo>;
  elevation(): Promise<ElevationState>;
  requestElevation(): Promise<string>;
  autostart(): Promise<AutostartState>;
  setAutostart(enabled: boolean): Promise<AutostartState>;
  shutdown(): void;
}

export interface ColorHubPort
  extends TelemetryPort,
    SettingsPort,
    FanPort,
    LightingPort,
    GpuPort,
    SetupPort,
    DriverPort,
    PawnioPort,
    SystemPort {}
