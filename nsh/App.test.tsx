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

test("the curve editor drags a point and writes the curve", async () => {
  const app = await open("./App.tsx");
  app.onDialog(() => "Write");
  await app.press("Fan control");
  await app.waitFor("Profiles");
  await app.press("id:curve-edit-sys");
  await app.drag("id:curve-sys", { x: 24, y: -18 });
  await app.press("id:curve-apply-sys");
  await app.wait(300);
  expect(app.texts().some((text) => text.includes("curve set to"))).toBe(true);
  expect(app.dialogs().length).toBeGreaterThan(0);
});
