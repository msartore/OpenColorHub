import { expect, test } from "nsh/test";

import { AppError } from "../errors";
import type { ActionResult, Setup, SetupKind, SetupList, Snapshot } from "../../domain/model";
import { applySetup, deleteSetup, listSetups, saveSetup } from "./setups";

const emptyList: SetupList = { setups: [], warning: null };
const snapshot = {} as Snapshot;

function fakePort() {
  const calls: { name?: string; id?: string; kind?: SetupKind } = {};
  return {
    calls,
    port: {
      async listSetups(): Promise<SetupList> {
        return emptyList;
      },
      async saveSetup(kind: SetupKind, name: string, id?: string): Promise<SetupList> {
        calls.kind = kind;
        calls.name = name;
        calls.id = id;
        return emptyList;
      },
      async applySetup(id: string): Promise<ActionResult> {
        calls.id = id;
        return { snapshot, message: "applied" };
      },
      async deleteSetup(kind: SetupKind, id: string): Promise<SetupList> {
        calls.kind = kind;
        calls.id = id;
        return emptyList;
      },
    },
  };
}

test("a preset name is required before anything is captured", async () => {
  const { port, calls } = fakePort();
  try {
    await saveSetup(port, "fans", "   ");
    throw new Error("expected the empty name to be refused");
  } catch (error) {
    expect(error instanceof AppError).toBe(true);
    expect((error as AppError).kind).toBe("invalid_name");
  }
  expect(calls.name).toBe(undefined);
});

test("a preset name is trimmed and an id updates it", async () => {
  const { port, calls } = fakePort();
  await saveSetup(port, "lighting", "  Night  ", "setup-1");
  expect(calls).toEqual({ kind: "lighting", name: "Night", id: "setup-1" });
});

test("list, apply and delete pass through", async () => {
  const { port, calls } = fakePort();
  await listSetups(port, "fans");
  const result = await applySetup(port, "s1");
  expect(result.message).toBe("applied");
  await deleteSetup(port, "fans", "s1");
  expect(calls.id).toBe("s1");
});

// Keeps the Setup shape used by the list visible to the type checker.
const shape: Setup = {
  id: "x",
  name: "x",
  kind: "fans",
  saved_at_ms: 0,
  fans: [],
  channels: [],
};
void shape;
