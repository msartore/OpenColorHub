import { expect, test } from "nsh/test";

import { createNativePort } from "./native";

/**
 * The native `requestElevation()` member returns a plain word ("launched" /
 * "declined"), not JSON. Regression: the adapter must hand it through as text
 * instead of running JSON.parse on it, which threw "Unexpected token launched".
 */
test("requestElevation passes the raw native outcome through", async () => {
  const outcomes = ["launched", "declined"];
  for (const outcome of outcomes) {
    const controller = { requestElevation: () => outcome };
    const port = createNativePort(controller as never);
    expect(await port.requestElevation()).toBe(outcome);
  }
});
