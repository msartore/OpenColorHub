/**
 * Fan rules, mirrored from `crates/colorctl-core/src/curve.rs` and
 * `settings.rs`. Rust still validates every write; these functions exist so a
 * screen can show the same answer before the round trip, and so the rules are
 * testable without a view.
 */

import type {
  CurvePoint,
  FanHeaderId,
  FanProfileId,
  FanStatus,
  Settings,
} from "./model";

export const CURVE_POINTS = 4;
export const MAX_TEMP_C = 120;
export const PUMP_FLOOR_PERCENT = 60;

export const DEFAULT_CURVE: readonly CurvePoint[] = [
  { temp_c: 20, percent: 20 },
  { temp_c: 40, percent: 40 },
  { temp_c: 60, percent: 60 },
  { temp_c: 80, percent: 100 },
];

export interface HeaderMeta {
  id: FanHeaderId;
  label: string;
  description: string;
}

export const FAN_HEADERS: readonly HeaderMeta[] = [
  { id: "sys", label: "SYS_FAN", description: "System fan / rear exhaust" },
  { id: "cpu", label: "CPU_FAN", description: "CPU cooler or AIO radiator" },
  { id: "aux0", label: "AUX0", description: "Auxiliary header" },
  { id: "aux1", label: "AUX1", description: "Auxiliary header" },
  { id: "aux2", label: "AUX2", description: "Auxiliary header" },
  { id: "aux3", label: "AUX3", description: "Auxiliary header" },
];

export const FAN_HEADER_IDS: readonly FanHeaderId[] = FAN_HEADERS.map(
  (header) => header.id,
);

export interface ProfileMeta {
  id: FanProfileId;
  label: string;
  blurb: string;
}

export const PROFILES: readonly ProfileMeta[] = [
  { id: "quiet", label: "Quiet", blurb: "20→80 °C · 20→85%" },
  { id: "standard", label: "Standard", blurb: "20→80 °C · 20→100%" },
  { id: "full", label: "Full tilt", blurb: "100% everywhere" },
];

/** Mirror of `FanProfile::points_for`. Always a fresh, mutable copy. */
export function curveFor(profile: FanProfileId, pump: boolean): CurvePoint[] {
  if (profile === "full") {
    return [
      { temp_c: 30, percent: 100 },
      { temp_c: 50, percent: 100 },
      { temp_c: 70, percent: 100 },
      { temp_c: 100, percent: 100 },
    ];
  }
  if (profile === "quiet") {
    return pump
      ? [
          { temp_c: 0, percent: 80 },
          { temp_c: 70, percent: 80 },
          { temp_c: 75, percent: 100 },
          { temp_c: 100, percent: 100 },
        ]
      : [
          { temp_c: 20, percent: 20 },
          { temp_c: 40, percent: 30 },
          { temp_c: 60, percent: 50 },
          { temp_c: 80, percent: 85 },
        ];
  }
  return pump
    ? [
        { temp_c: 0, percent: 80 },
        { temp_c: 65, percent: 80 },
        { temp_c: 70, percent: 100 },
        { temp_c: 100, percent: 100 },
      ]
    : [
        { temp_c: 20, percent: 20 },
        { temp_c: 40, percent: 40 },
        { temp_c: 60, percent: 60 },
        { temp_c: 80, percent: 100 },
      ];
}

export function isPumpHeader(settings: Settings, header: FanHeaderId): boolean {
  return settings.pump_header === header;
}

/** Mirror of `Settings::clamp_duty`: the pump floor, then 0–100. */
export function clampDuty(percent: number, pump: boolean): number {
  const floor = pump ? PUMP_FLOOR_PERCENT : 0;
  return Math.min(100, Math.max(floor, Math.round(percent)));
}

/** Mirror of `curve::eval`: piecewise linear, clamped at the ends. */
export function evaluateCurve(curve: readonly CurvePoint[], tempC: number): number {
  if (curve.length === 0) return 0;
  const first = curve[0];
  const last = curve[curve.length - 1];
  if (tempC <= first.temp_c) return first.percent;
  if (tempC >= last.temp_c) return last.percent;
  for (let index = 1; index < curve.length; index += 1) {
    const a = curve[index - 1];
    const b = curve[index];
    if (tempC >= a.temp_c && tempC <= b.temp_c) {
      const span = Math.max(1e-6, b.temp_c - a.temp_c);
      const t = (tempC - a.temp_c) / span;
      return Math.round(Math.min(100, Math.max(0, a.percent + t * (b.percent - a.percent))));
    }
  }
  return last.percent;
}

/** Equality with a 1-point tolerance, since the chip round-trips through PWM. */
export function sameCurve(
  a: readonly CurvePoint[] | null,
  b: readonly CurvePoint[],
  tolerance = 1,
): boolean {
  if (!a || a.length !== b.length) return false;
  return a.every(
    (point, index) =>
      point.temp_c === b[index].temp_c &&
      Math.abs(point.percent - b[index].percent) <= tolerance,
  );
}

export type CurveProblemKind = "count" | "percent" | "temperature" | "order";

export interface CurveProblem {
  kind: CurveProblemKind;
  message: string;
}

/** Mirror of `curve::validate`. Returns the first problem, or null. */
export function validateCurve(curve: readonly CurvePoint[]): CurveProblem | null {
  if (curve.length !== CURVE_POINTS) {
    return {
      kind: "count",
      message: `a curve has exactly ${CURVE_POINTS} points — got ${curve.length}`,
    };
  }
  for (const point of curve) {
    if (point.percent < 0 || point.percent > 100) {
      return { kind: "percent", message: `duty ${point.percent}% is outside 0–100%` };
    }
    if (point.temp_c < 0 || point.temp_c > MAX_TEMP_C) {
      return {
        kind: "temperature",
        message: `temperature ${point.temp_c} °C is outside 0–${MAX_TEMP_C} °C`,
      };
    }
  }
  for (let index = 1; index < curve.length; index += 1) {
    if (curve[index].temp_c <= curve[index - 1].temp_c) {
      return {
        kind: "order",
        message: `temperatures must rise: ${curve[index].temp_c} °C follows ${curve[index - 1].temp_c} °C`,
      };
    }
  }
  return null;
}

export function curveSignature(curve: readonly CurvePoint[] | null): string {
  return curve ? curve.map((point) => `${point.temp_c}:${point.percent}`).join(",") : "";
}

/** A profile is active when every detected fan sits on its own curve. */
export function isProfileActive(
  presentFans: readonly FanStatus[],
  profile: FanProfileId,
): boolean {
  return (
    presentFans.length > 0 &&
    presentFans.every((fan) => sameCurve(fan.curve, curveFor(profile, fan.pump)))
  );
}

export interface DutyWarnings {
  stalled: FanStatus[];
  lowDuty: FanStatus[];
}

/** A bound pump is exempt from the low-duty warning: its floor is enforced. */
export function dutyWarnings(
  presentFans: readonly FanStatus[],
  lowDutyThreshold: number,
): DutyWarnings {
  return {
    stalled: presentFans.filter((fan) => fan.stalled),
    lowDuty: presentFans.filter((fan) => !fan.pump && fan.percent < lowDutyThreshold),
  };
}
