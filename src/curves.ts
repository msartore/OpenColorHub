/** Built-in SmartFan profiles, shared by the UI and the browser demo. */

import type { CurvePoint, FanProfileId } from "./types";

export interface ProfileMeta {
  id: FanProfileId;
  label: string;
  blurb: string;
}

export const PROFILES: ProfileMeta[] = [
  { id: "quiet", label: "Quiet", blurb: "20→80 °C · 20→85%" },
  { id: "standard", label: "Standard", blurb: "20→80 °C · 20→100%" },
  { id: "full", label: "Full tilt", blurb: "100% everywhere" },
];

/** Mirror of `FanProfile::points_for` in the core crate. */
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

/** Equality with a 1-point tolerance, since the chip round-trips through PWM. */
export function sameCurve(a: CurvePoint[] | null, b: CurvePoint[]): boolean {
  if (!a || a.length !== b.length) return false;
  return a.every(
    (point, index) =>
      point.temp_c === b[index].temp_c && Math.abs(point.percent - b[index].percent) <= 1,
  );
}
