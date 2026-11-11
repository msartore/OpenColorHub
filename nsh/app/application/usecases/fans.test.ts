import { expect, test } from "nsh/test";

import { AppError } from "../errors";
import type { ActionResult, CurvePoint, FanHeaderId, FanProfileId, Snapshot } from "../../domain/model";
import { curveFor } from "../../domain/fan";
import { applyFanProfile, setFanCurve, setFanDuty } from "./fans";

const snapshot = {} as Snapshot;

function fakePort() {
  const calls: {
    percent?: { header: FanHeaderId; percent: number };
    curve?: { header: FanHeaderId; points: CurvePoint[] };
    profile?: { headers: FanHeaderId[]; profile: FanProfileId };
  } = {};

  return {
    calls,
    port: {
      async setFanPercent(header: FanHeaderId, percent: number): Promise<ActionResult> {
        calls.percent = { header, percent };
        return { snapshot, message: "set" };
      },
      async setFanCurve(header: FanHeaderId, points: CurvePoint[]): Promise<ActionResult> {
        calls.curve = { header, points };
        return { snapshot, message: "curve" };
      },
      async applyProfile(
        headers: FanHeaderId[],
        profile: FanProfileId,
      ): Promise<ActionResult> {
        calls.profile = { headers, profile };
        return { snapshot, message: "profile" };
      },
    },
  };
}

test("a pump duty is raised to the floor before the port sees it", async () => {
  const { port, calls } = fakePort();
  await setFanDuty(port, { header: "aux3", percent: 10, pump: true });
  expect(calls.percent).toEqual({ header: "aux3", percent: 60 });
});

test("an unbound header keeps its value", async () => {
  const { port, calls } = fakePort();
  await setFanDuty(port, { header: "sys", percent: 37, pump: false });
  expect(calls.percent).toEqual({ header: "sys", percent: 37 });
});

test("a malformed curve never reaches the port", async () => {
  const { port, calls } = fakePort();
  await expect(
    setFanCurve(port, {
      header: "cpu",
      points: curveFor("standard", false).slice(0, 3),
      pump: false,
    }),
  ).rejects.toMatchObject({ name: "AppError", message: "a curve has exactly 4 points — got 3" });
  expect(calls.curve).toBe(undefined);
});

test("a pump curve below the floor is refused", async () => {
  const { port, calls } = fakePort();
  const points = curveFor("standard", true).map((point) => ({ ...point, percent: 20 }));
  try {
    await setFanCurve(port, { header: "aux3", points, pump: true });
    throw new Error("expected the pump curve to be refused");
  } catch (error) {
    expect(error instanceof AppError).toBe(true);
    expect((error as AppError).kind).toBe("invalid_curve");
  }
  expect(calls.curve).toBe(undefined);
});

test("a profile goes to every header it was given", async () => {
  const { port, calls } = fakePort();
  await applyFanProfile(port, { headers: ["sys", "cpu"], profile: "quiet" });
  expect(calls.profile).toEqual({ headers: ["sys", "cpu"], profile: "quiet" });
});
