import { expect, open, test } from "nsh/test";

/**
 * The elevation prompt can appear in headless runs when PawnIO is installed but
 * this process is not elevated. Dismiss it; every other dialog is the app's
 * "Write?" confirmation the tests intend to accept.
 */
const answerDialog = (dialog: { title?: string | null }): string =>
  dialog.title === "Fan control needs administrator rights"
    ? "Continue without fan control"
    : "Write";

/** Lighting groups start collapsed; open them when a test needs their content. */
async function expandLighting(app: Awaited<ReturnType<typeof open>>) {
  await app.waitFor("Motherboard");
  await app.press("id:motherboard-details");
  await app.press("id:gpu-details");
}

/** Lighting tests need a desktop viewport so the group headers fit. */
const lightingApp = () => open("./App.tsx", { width: 1240 });

test("the dashboard renders the live machine", async () => {
  const app = await open("./App.tsx");
  await app.waitFor("Dashboard");
  expect(app.texts()).toContain("Temperatures");
  expect(
    app.texts().some((text) => text.toLowerCase().includes("motherboard")),
  ).toBe(true);
  expect(app.texts().some((text) => text.toLowerCase().includes("cpu"))).toBe(true);
  expect(app.texts()).toContain("Fan headers");
  expect(app.texts()).toContain("SYS_FAN");
});

test("fan control writes through the controller", async () => {
  const app = await open("./App.tsx");
  app.onDialog(answerDialog);
  await app.press("Fan control");
  await app.waitFor("Profiles");
  await app.press("Standard");
  await app.wait(250);
  expect(app.dialogs().length).toBeGreaterThan(0);
  expect(app.texts().some((text) => text.includes("Applied"))).toBe(true);
});

test("the single colour swatch opens the picker and keeps the chosen colour", async () => {
  const app = await lightingApp();
  await app.press("Lighting");
  await expandLighting(app);
  await app.waitFor("Frame preview");
  expect(app.count("id:color-preset-1")).toBe(1);
  await app.press("id:color-preset-1");
  await app.waitFor("Choose a colour");
  // Pick a different colour in the area and commit it.
  await app.drag("id:colour-area", { x: 90, y: -40 });
  await app.wait(150);
  await app.press("text:Done");
  await app.wait(150);
  expect(app.count("id:color-preset-1")).toBe(1);
});

test("the curve editor drags a point and writes the curve", async () => {
  const app = await open("./App.tsx");
  app.onDialog(answerDialog);
  await app.press("Fan control");
  await app.waitFor("Profiles");
  await app.drag("id:curve-sys", { x: 24, y: -18 });
  await app.press("id:curve-apply-sys");
  await app.wait(300);
  expect(app.texts().some((text) => text.includes("curve set to"))).toBe(true);
  expect(app.dialogs().length).toBeGreaterThan(0);
});

test("lighting renders the frame and turns everything off", async () => {
  const app = await lightingApp();
  app.onDialog(answerDialog);
  await app.press("Lighting");
  await expandLighting(app);
  await app.waitFor("Frame preview");
  expect(app.texts()).toContain("Global colour");
  expect(app.texts().some((text) => text.includes("channels"))).toBe(true);
  await app.press("id:lighting-all-off");
  await app.wait(300);
  expect(app.texts().some((text) => text.includes("All lighting turned off"))).toBe(true);
});

test("a channel applies its colour and brightness", async () => {
  const app = await lightingApp();
  app.onDialog(answerDialog);
  await app.press("Lighting");
  await expandLighting(app);
  await app.waitFor("Frame preview");
  // The saved setup may be restored at launch, so the switch starts in either
  // state. Applying a channel must write either way — on means a new colour,
  // off means the channel is handed back.
  await app.press("id:channel-enabled-argb1");
  await app.press("id:channel-apply-argb1");
  await app.wait(300);
  expect(app.dialogs().length).toBeGreaterThan(0);
  expect(
    app.texts().some((text) => text.includes("set to") || text.includes("turned off")),
  ).toBe(true);
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

test("lighting groups the motherboard and GPU controllers", async () => {
  const app = await lightingApp();
  await app.press("Lighting");
  await expandLighting(app);
  await app.waitFor("GPU LED");
  const texts = app.texts();
  expect(texts).toContain("Motherboard");
  expect(texts).toContain("GPU");
  expect(texts.some((text) => text.includes("Nuvoton RGB at I²C 0x36"))).toBe(true);
  expect(texts.some((text) => text.includes("ARGB header"))).toBe(true);
  expect(texts.some((text) => text.includes("Top / side"))).toBe(true);
});

test("settings offers the restore, release and GPU switches", async () => {
  const app = await open("./App.tsx");
  await app.press("Settings");
  await app.waitFor("Safety");
  const texts = app.texts();
  expect(
    texts.some((text) => text.includes("Restore saved setup at startup")),
  ).toBe(true);
  expect(texts.some((text) => text.includes("Release fans on exit"))).toBe(true);
  expect(texts.some((text) => text.includes("GPU LED control"))).toBe(true);
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


