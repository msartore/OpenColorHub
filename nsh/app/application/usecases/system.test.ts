import { expect, test } from "nsh/test";

import type { AutostartState, Settings, SettingsPatch } from "../../domain/model";
import { DEFAULT_SETTINGS } from "../../domain/settings";
import { setAutostart, shouldOfferElevation } from "./system";

function fakePort() {
  const calls: { enabled?: boolean; patch?: SettingsPatch } = {};
  const state = (enabled: boolean): AutostartState => ({
    supported: true,
    enabled,
    mechanism: "logon-task",
    detail: "HKCU\\...\\Run",
  });

  return {
    calls,
    port: {
      async setAutostart(enabled: boolean): Promise<AutostartState> {
        calls.enabled = enabled;
        return state(enabled);
      },
      async saveSettings(patch: SettingsPatch): Promise<Settings> {
        calls.patch = patch;
        return {
          ...DEFAULT_SETTINGS,
          ...patch,
          pump_header:
            patch.pump_header === "none"
              ? null
              : (patch.pump_header ?? DEFAULT_SETTINGS.pump_header),
        };
      },
    },
  };
}

test("enabling autostart also turns on re-apply at launch", async () => {
  const { port, calls } = fakePort();
  const outcome = await setAutostart(port, true);
  expect(calls.enabled).toBe(true);
  expect(calls.patch).toEqual({ reapply_led_on_launch: true });
  expect(outcome.settings?.reapply_led_on_launch).toBe(true);
  expect(outcome.state.enabled).toBe(true);
});

test("disabling autostart leaves the lighting preference alone", async () => {
  const { port, calls } = fakePort();
  const outcome = await setAutostart(port, false);
  expect(calls.enabled).toBe(false);
  expect(calls.patch).toBe(undefined);
  expect(outcome.settings).toBe(null);
});

test("elevation is offered only when PawnIO is blocked", () => {
  const elevation = (elevated: boolean, canRequest = true) => ({
    elevated,
    can_request: canRequest,
    detail: "",
  });
  expect(
    shouldOfferElevation(elevation(false), { state: "denied", detail: "" }),
  ).toBe(true);
  expect(
    shouldOfferElevation(elevation(false), { state: "ready", version: "2.2.0" }),
  ).toBe(false);
  expect(
    shouldOfferElevation(elevation(false), { state: "not_installed", detail: "" }),
  ).toBe(false);
  expect(
    shouldOfferElevation(elevation(true), { state: "denied", detail: "" }),
  ).toBe(false);
  expect(
    shouldOfferElevation(elevation(false, false), { state: "denied", detail: "" }),
  ).toBe(false);
});
