/**
 * Catalogue policy. The adapter does the network work; this is the rule the
 * screen used to carry: a cached answer is shown, then refreshed once in the
 * background.
 */

import type { DriverList } from "../../domain/model";
import type { DriverPort } from "../ports";

export function loadDrivers(port: DriverPort, refresh: boolean): Promise<DriverList> {
  return port.drivers(refresh);
}

export function backgroundRefresh(list: DriverList): boolean {
  return list.cached;
}
