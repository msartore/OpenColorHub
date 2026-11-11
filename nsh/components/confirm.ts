/** The one confirmation gate for hardware writes. */

import { AlertDialog } from "nsh/base";

import type { Settings } from "../app/domain/model";

export async function confirmWrite(settings: Settings, message: string): Promise<boolean> {
  if (!settings.confirm_writes) return true;
  const answer = await AlertDialog.show({
    title: "Confirm hardware write",
    description: message,
    actions: [
      { id: "cancel", label: "Cancel", role: "cancel" },
      { id: "write", label: "Write", role: "destructive" },
    ],
  });
  return answer.action === "write";
}
