import { expect, open, test } from "nsh/test";

test("the dashboard renders the live machine", async () => {
  const app = await open("./App.tsx");
  await app.waitFor("Dashboard");
  expect(app.texts()).toContain("Motherboard temperature");
  expect(app.texts()).toContain("Fan headers");
  expect(app.texts()).toContain("SYS_FAN");
});

test("fan control writes through the controller", async () => {
  const app = await open("./App.tsx");
  app.onDialog(() => "Write");
  await app.press("Fan control");
  await app.waitFor("Profiles");
  await app.press("Standard");
  await app.wait(250);
  expect(app.dialogs().length).toBeGreaterThan(0);
  expect(app.texts().some((text) => text.includes("Applied"))).toBe(true);
});

test("the colour dialog writes the selected colour", async () => {
  const app = await open("./App.tsx");
  app.onDialog(() => "Write");
  await app.press("Lighting");
  await app.waitFor("Frame preview");
  await app.press("Change");
  await app.press("id:color-preset-1");
  await app.press("Done");
  await app.press("id:lighting-apply-all");
  await app.wait(350);
  expect(app.texts().some((text) => text.includes("#e879f9"))).toBe(true);
});

test("the curve editor drags a point and writes the curve", async () => {
  const app = await open("./App.tsx");
  app.onDialog(() => "Write");
  await app.press("Fan control");
  await app.waitFor("Profiles");
  await app.drag("id:curve-sys", { x: 24, y: -18 });
  await app.press("id:curve-apply-sys");
  await app.wait(300);
  expect(app.texts().some((text) => text.includes("curve set to"))).toBe(true);
  expect(app.dialogs().length).toBeGreaterThan(0);
});

test("lighting renders the frame and turns everything off", async () => {
  const app = await open("./App.tsx");
  app.onDialog(() => "Write");
  await app.press("Lighting");
  await app.waitFor("Frame preview");
  expect(app.texts()).toContain("Global colour");
  expect(app.texts().some((text) => text.includes("channels"))).toBe(true);
  await app.press("id:lighting-all-off");
  await app.wait(300);
  expect(app.texts().some((text) => text.includes("All lighting turned off"))).toBe(true);
});

test("a channel applies its colour and brightness", async () => {
  const app = await open("./App.tsx");
  app.onDialog(() => "Write");
  await app.press("Lighting");
  await app.waitFor("Frame preview");
  await app.press("id:channel-enabled-argb1");
  await app.press("id:channel-apply-argb1");
  await app.wait(300);
  expect(app.dialogs().length).toBeGreaterThan(0);
  expect(app.texts().some((text) => text.includes("set to"))).toBe(true);
});

test("settings shows the backend, safety and paths", async () => {
  const app = await open("./App.tsx");
  await app.press("Settings");
  await app.waitFor("Telemetry");
  expect(app.texts()).toContain("Backend");
  expect(app.texts()).toContain("Safety");
  expect(app.texts()).toContain("Files & paths");
  expect(app.texts().some((text) => text.includes("Pump channel"))).toBe(true);
});

test("about reports the detected board and build", async () => {
  const app = await open("./App.tsx");
  await app.press("About");
  await app.waitFor("Detected motherboard");
  expect(app.texts()).toContain("Build");
  expect(app.texts()).toContain("Upstream");
  expect(app.texts().some((text) => text.includes("CVN"))).toBe(true);
});

test("a fan preset saves from the dialog", async () => {
  const app = await open("./App.tsx");
  await app.press("Fan control");
  await app.waitFor("Profiles");
  await app.press("id:fan-presets");
  await app.waitFor("Fan presets");
  await app.type("Preset name", "Bench");
  await app.press("Save preset");
  await app.wait(300);
  expect(app.texts().some((text) => text.includes("saved"))).toBe(true);
  expect(app.texts()).toContain("Bench");
});

test("the driver catalogue groups vendor files", async () => {
  const app = await open("./App.tsx");
  await app.press("Drivers");
  await app.waitFor("Firmware");
  expect(app.texts()).toContain("BIOS 1.0.7");
  expect(app.texts()).toContain("Drivers");
  expect(app.texts()).toContain("Manuals");
  expect(app.texts().some((text) => text.includes("Show all 6 firmware"))).toBe(true);
  expect(app.texts().some((text) => text.includes("Firmware update available"))).toBe(true);
});
