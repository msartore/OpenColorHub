/**
 * Preset commands. A preset covers one half of the machine; the name rule is
 * the only policy here, and it runs before anything is captured.
 */

import type { ActionResult, SetupKind, SetupList } from "../../domain/model";
import { normalizeSetupName } from "../../domain/setup";
import { AppError } from "../errors";
import type { SetupPort } from "../ports";

export function listSetups(port: SetupPort, kind: SetupKind): Promise<SetupList> {
  return port.listSetups(kind);
}

export async function saveSetup(
  port: SetupPort,
  kind: SetupKind,
  rawName: string,
  id?: string,
): Promise<SetupList> {
  const name = normalizeSetupName(rawName);
  if (!name) throw new AppError("invalid_name", "Give the preset a name first.");
  return port.saveSetup(kind, name, id);
}

export function applySetup(port: SetupPort, id: string): Promise<ActionResult> {
  return port.applySetup(id);
}

export function deleteSetup(port: SetupPort, kind: SetupKind, id: string): Promise<SetupList> {
  return port.deleteSetup(kind, id);
}
