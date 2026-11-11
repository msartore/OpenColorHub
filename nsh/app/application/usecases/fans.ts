/**
 * Fan writes: the guard rails the UI can enforce before the round trip. The
 * core repeats every check; these exist so a bad value never leaves the app
 * and so the rule is testable without a view.
 */

import type { ActionResult, CurvePoint, FanHeaderId, FanProfileId } from "../../domain/model";
import { PUMP_FLOOR_PERCENT, clampDuty, validateCurve } from "../../domain/fan";
import { AppError } from "../errors";
import type { FanPort } from "../ports";

export interface SetDutyInput {
  header: FanHeaderId;
  percent: number;
  pump: boolean;
}

export async function setFanDuty(
  port: FanPort,
  input: SetDutyInput,
): Promise<ActionResult> {
  return port.setFanPercent(input.header, clampDuty(input.percent, input.pump));
}

export interface SetCurveInput {
  header: FanHeaderId;
  points: CurvePoint[];
  pump: boolean;
}

export async function setFanCurve(
  port: FanPort,
  input: SetCurveInput,
): Promise<ActionResult> {
  const problem = validateCurve(input.points);
  if (problem) throw new AppError("invalid_curve", problem.message);

  if (input.pump) {
    const low = input.points.filter((point) => point.percent < PUMP_FLOOR_PERCENT);
    if (low.length > 0) {
      throw new AppError(
        "invalid_curve",
        `a pump curve must stay at or above ${PUMP_FLOOR_PERCENT}% — ${low[0].percent}% would stall it`,
      );
    }
  }

  return port.setFanCurve(input.header, input.points.map((point) => ({ ...point })));
}

export interface ApplyProfileInput {
  headers: readonly FanHeaderId[];
  profile: FanProfileId;
}

export async function applyFanProfile(
  port: FanPort,
  input: ApplyProfileInput,
): Promise<ActionResult> {
  if (input.headers.length === 0) {
    throw new AppError("invalid_curve", "no fan headers to apply the profile to");
  }
  return port.applyProfile([...input.headers], input.profile);
}
