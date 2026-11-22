import { expect, open, test } from "nsh/test";

/** A failed download keeps its reason on the row and offers a retry, instead
 * of vanishing the moment the transient toast disappears. */
test("a failed driver download stays visible with a retry", async () => {
  const app = await open("../test-fixtures/drivers-progress-entry.tsx");
  await app.waitFor("Failing driver");
  await app.press("id:driver-download-failing-driver");
  await app.wait(150);
  expect(app.texts().some((text) => text.includes("network unreachable"))).toBe(true);
  expect(app.texts()).toContain("Retry");
});
