import { expect, test } from "nsh/test";

import type { ActionResult, ChannelId, Snapshot } from "../../domain/model";
import type { Rgb } from "../../domain/lighting";
import { allOff, dimChannels, setChannel } from "./lighting";

const snapshot = {} as Snapshot;

function fakePort() {
  const calls: {
    channel?: { id: ChannelId; color: Rgb; brightness: number; enabled: boolean };
    all?: { color: Rgb; brightness: number; enabled: boolean };
    brightness?: number;
  } = {};

  return {
    calls,
    port: {
      async setChannel(
        id: ChannelId,
        color: Rgb,
        brightness: number,
        enabled: boolean,
      ): Promise<ActionResult> {
        calls.channel = { id, color, brightness, enabled };
        return { snapshot, message: "channel" };
      },
      async setAllChannels(color: Rgb, brightness: number, enabled: boolean): Promise<ActionResult> {
        calls.all = { color, brightness, enabled };
        return { snapshot, message: "all" };
      },
      async setBrightness(percent: number): Promise<ActionResult> {
        calls.brightness = percent;
        return { snapshot, message: "brightness" };
      },
      async turnOffChannel(): Promise<ActionResult> {
        return { snapshot, message: "off" };
      },
      async applyLed(): Promise<ActionResult> {
        return { snapshot, message: "applied" };
      },
    },
  };
}

test("a channel write is clamped before the port sees it", async () => {
  const { port, calls } = fakePort();
  await setChannel(port, {
    channel: "argb1",
    color: [300, 12, -4],
    brightness: 140,
    enabled: true,
  });
  expect(calls.channel).toEqual({
    id: "argb1",
    color: [255, 12, 0],
    brightness: 100,
    enabled: true,
  });
});

test("all off sends black at zero, disabled", async () => {
  const { port, calls } = fakePort();
  await allOff(port);
  expect(calls.all).toEqual({ color: [0, 0, 0], brightness: 0, enabled: false });
});

test("dim clamps its percentage", async () => {
  const { port, calls } = fakePort();
  await dimChannels(port, 250);
  expect(calls.brightness).toBe(100);
  await dimChannels(port, -5);
  expect(calls.brightness).toBe(0);
});
