import { expect, test } from "nsh/test";

import { clampDuty, curveFor, evaluateCurve, isProfileActive, validateCurve } from "./fan";
import type { FanStatus } from "./model";

test("profiles mirror the core's tables", () => {
  expect(curveFor("quiet", false)).toEqual([
    { temp_c: 20, percent: 20 },
    { temp_c: 40, percent: 30 },
    { temp_c: 60, percent: 50 },
    { temp_c: 80, percent: 85 },
  ]);
  expect(curveFor("standard", false)).toEqual([
    { temp_c: 20, percent: 20 },
    { temp_c: 40, percent: 40 },
    { temp_c: 60, percent: 60 },
    { temp_c: 80, percent: 100 },
  ]);
  expect(curveFor("full", true)[0]).toEqual({ temp_c: 30, percent: 100 });
  expect(curveFor("quiet", true)).toEqual([
    { temp_c: 0, percent: 80 },
    { temp_c: 70, percent: 80 },
    { temp_c: 75, percent: 100 },
    { temp_c: 100, percent: 100 },
  ]);
});

test("a pump is never commanded below its floor", () => {
  expect(clampDuty(10, true)).toBe(60);
  expect(clampDuty(0, false)).toBe(0);
  expect(clampDuty(120, false)).toBe(100);
  expect(clampDuty(59.6, true)).toBe(60);
});

test("curve evaluation interpolates and clamps at both ends", () => {
  const curve = curveFor("standard", false);
  expect(evaluateCurve(curve, 0)).toBe(20);
  expect(evaluateCurve(curve, 20)).toBe(20);
  expect(evaluateCurve(curve, 30)).toBe(30);
  expect(evaluateCurve(curve, 50)).toBe(50);
  expect(evaluateCurve(curve, 70)).toBe(80);
  expect(evaluateCurve(curve, 90)).toBe(100);
});

test("validation rejects the same shapes the core rejects", () => {
  expect(validateCurve(curveFor("standard", false))).toBe(null);
  expect(validateCurve(curveFor("standard", false).slice(0, 3))?.kind).toBe("count");

  const flat = curveFor("standard", false);
  flat[1] = { temp_c: 20, percent: 40 };
  expect(validateCurve(flat)?.kind).toBe("order");

  const fast = curveFor("standard", false);
  fast[3] = { temp_c: 80, percent: 101 };
  expect(validateCurve(fast)?.kind).toBe("percent");
});

test("a profile is active only when every detected fan is on it", () => {
  const fan = (header: "sys" | "cpu", curve: FanStatus["curve"], pump = false): FanStatus => ({
    header,
    detected: true,
    stalled: false,
    label: header,
    description: "",
    bank: 1,
    pump,
    rpm: 1000,
    pwm: 128,
    percent: 50,
    mode: "smart_fan",
    curve,
    curve_percent: 50,
  });

  expect(
    isProfileActive(
      [fan("sys", curveFor("standard", false)), fan("cpu", curveFor("standard", false))],
      "standard",
    ),
  ).toBe(true);
  expect(isProfileActive([fan("sys", curveFor("quiet", false))], "standard")).toBe(false);
});
