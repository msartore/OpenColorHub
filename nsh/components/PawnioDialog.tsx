/**
 * PawnIO prompt. Fan control on Windows needs the vendor's signed driver; it is
 * downloaded from the vendor's own release page, checked against a pinned
 * SHA-256, and run with its own UI and consent prompts.
 */

import { Application } from "nsh/desktop";
import { Text, View, signal } from "nsh";
import { Dialog, Progress } from "nsh/base";
import { openUrl } from "nsh";
import type { Accessor } from "nsh";

import type { AppStore } from "../app/composition/store";
import type { PawnioProgress, PawnioStatus } from "../app/domain/model";
import { dialogWidth } from "../app/presentation/layout";
import { centeredDialog } from "./ui";

type Tone = "ok" | "warn" | "danger" | "muted";

const PIN_NOTE =
  "PawnIO 2.2.0 · downloaded from the project's GitHub releases and checked against a pinned SHA-256 before it runs.";

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function Action(props: {
  label: string;
  primary?: boolean;
  disabled?: boolean;
  onPress: () => void;
}) {
  return (
    <View
      role="button"
      class={`rounded-[6px] px-2.5 py-1 ${
        props.disabled
          ? "border border-border opacity-50"
          : props.primary
            ? "bg-primary"
            : "border border-border"
      }`}
      onPress={() => {
        if (!props.disabled) props.onPress();
      }}
    >
      <Text class={`text-[11px] ${props.primary ? "text-primary-foreground" : "text-foreground"}`}>
        {props.label}
      </Text>
    </View>
  );
}

export function PawnioDialog(props: {
  status: PawnioStatus;
  open: Accessor<boolean>;
  onOpenChange: (open: boolean) => void;
  onStatus: (status: PawnioStatus) => void;
  store: AppStore;
  toast: (message: string, tone?: Tone) => void;
}) {
  const busy = signal(false);
  const progress = signal<PawnioProgress | null>(null);
  const line = signal("");
  const note = signal<string | null>(null);

  const denied = () => props.status.state === "denied";
  const percent = (): number => {
    const current = progress();
    if (!current || !current.total) return 0;
    return Math.min(100, Math.round((current.received / current.total) * 100));
  };

  const elevate = async (): Promise<void> => {
    busy.set(true);
    line.set("Restarting with administrator rights…");
    try {
      const outcome = await props.store.system.requestElevation();
      if (outcome === "launched") Application.quit();
      else line.set("Elevation cancelled — still running as a standard user.");
    } catch (error) {
      line.set(`Could not elevate: ${String(error)}`);
    } finally {
      busy.set(false);
    }
  };

  const install = async (): Promise<void> => {
    busy.set(true);
    progress.set(null);
    note.set(null);
    line.set("Downloading the installer…");
    try {
      const next = await props.store.pawnio.install((frame) => {
        progress.set(frame);
        line.set(
          frame.total
            ? `Downloading the installer… ${Math.min(100, Math.round((frame.received / frame.total) * 100))}%`
            : `Downloading the installer… ${formatBytes(frame.received)}`,
        );
      });
      props.onStatus(next);
      if (next.state === "ready") {
        line.set(`PawnIO ${next.version} is installed — re-reading the hardware…`);
        await props.store.refresh();
        props.toast("PawnIO installed — fan readings are live", "ok");
        props.onOpenChange(false);
      } else if (next.state === "denied") {
        line.set("Installed, but this process is not elevated. Restart as administrator.");
      } else if (next.state === "failed") {
        line.set(`Still unavailable: ${next.detail}`);
      } else {
        line.set("The installer finished, but the driver is not answering.");
      }
    } catch (error) {
      line.set(`Could not install: ${String(error)}`);
    } finally {
      busy.set(false);
    }
  };

  return (
    <Dialog
      open={props.open()}
      onOpenChange={({ open }) => props.onOpenChange(open)}
      accessibilityLabel="Fan control needs PawnIO"
      class="rounded-[12px] border border-border bg-popover p-4 gap-3"
      style={centeredDialog(dialogWidth(520))}
      backdropClass="bg-black/40"
    >
      <Text class="text-base font-semibold text-foreground">Fan control needs PawnIO</Text>
      <Text class="text-xs text-muted-foreground">
        PawnIO is a signed, sandboxed I/O driver. OpenColorHub uses it to read fan
        speeds and temperatures on Windows; without it those readings are simulated.
      </Text>
      <View class="gap-1">
        <Text class="text-[11px] text-muted-foreground">
          · Only Super I/O ports are reachable through it — no physical memory, no MSRs.
        </Text>
        <Text class="text-[11px] text-muted-foreground">· {PIN_NOTE}</Text>
        <Text class="text-[11px] text-muted-foreground">
          · The installer runs with its own window and its own administrator prompt.
        </Text>
      </View>

      <Text class={`text-[11px] ${denied() ? "text-[#fbbf24]" : "text-muted-foreground"}`}>
        {denied()
          ? "PawnIO is installed, but this process is not elevated. Restart OpenColorHub as administrator."
          : "This is a separate project (GPL-2.0) by namazso, not bundled with OpenColorHub."}
      </Text>

      <If when={line()}>
        <Text class="text-[11px] font-mono text-muted-foreground">{line()}</Text>
      </If>
      <If when={busy() && progress()}>
        <View class="gap-1">
          <Progress
            value={percent()}
            indeterminate={!progress()?.total}
            class="h-1"
            trackClass="h-1 rounded-full bg-muted"
            indicatorClass="h-1 rounded-full bg-primary"
          />
          <Text class="text-[10px] text-muted-foreground">
            {progress()?.total
              ? `${formatBytes(progress()!.received)} of ${formatBytes(progress()!.total!)}`
              : `${formatBytes(progress()!.received)} received`}
          </Text>
        </View>
      </If>
      <If when={note()}>
        {(message) => <Text class="text-[11px] text-[#f87171]">{message()}</Text>}
      </If>

      <View class="flex-row items-center gap-2" style={{ flexWrap: "wrap" }}>
        <Action
          label="pawnio.eu"
          onPress={() => {
            void openUrl("https://pawnio.eu").catch((error) => note.set(String(error)));
          }}
        />
        <Action label="Not now" onPress={() => props.onOpenChange(false)} />
        <Action
          label={denied() ? "Restart as administrator" : "Install PawnIO"}
          primary
          disabled={busy()}
          onPress={() => void (denied() ? elevate() : install())}
        />
      </View>
    </Dialog>
  );
}
