import { expect, test } from "nsh/test";

import {
  DRIVER_KIND_ORDER,
  progressPercent,
  rowsOfKind,
  shouldRefreshInBackground,
  visibleRows,
} from "./drivers";
import type { DriverRow } from "./model";

function file(index: number, kind: DriverRow["kind"] = "bios"): DriverRow {
  return {
    title: `BIOS ${index}`,
    kind,
    url: "https://example/colorful.cn/x.zip",
    file_name: `bios-${index}.zip`,
    published: "2025-01-01",
    newer: index === 4,
  };
}

test("firmware comes first, then drivers, utilities, manuals and other", () => {
  expect(DRIVER_KIND_ORDER).toEqual(["bios", "driver", "utility", "manual", "other"]);
  const rows = [file(1, "manual"), file(2, "driver"), file(3, "bios")];
  expect(rowsOfKind(rows, "bios").length).toBe(1);
  expect(rowsOfKind(rows, "manual")[0].file_name).toBe("bios-1.zip");
});

test("only firmware collapses past the visible limit", () => {
  const bios = [file(1), file(2), file(3), file(4), file(5), file(6)];
  const collapsed = visibleRows("bios", bios, false);
  expect(collapsed.shown.length).toBe(4);
  expect(collapsed.hiddenCount).toBe(2);
  expect(collapsed.collapsed).toBe(true);

  expect(visibleRows("bios", bios, true).shown.length).toBe(6);
  const drivers = bios.map((row) => ({ ...row, kind: "driver" as const }));
  expect(visibleRows("driver", drivers, false).shown.length).toBe(6);
});

test("progress never invents a total", () => {
  expect(progressPercent(50, 200)).toBe(25);
  expect(progressPercent(200, 200)).toBe(100);
  expect(progressPercent(250, 200)).toBe(100);
  expect(progressPercent(10, null)).toBe(null);
  expect(progressPercent(10, 0)).toBe(null);
});

test("a cached answer is refreshed in the background", () => {
  expect(shouldRefreshInBackground(true)).toBe(true);
  expect(shouldRefreshInBackground(false)).toBe(false);
});
