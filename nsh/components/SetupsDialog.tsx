/**
 * Saved presets for one half of the machine. A fan preset never writes
 * lighting and a lighting preset never writes fans.
 */

import { Text, View, effect, signal } from "nsh";
import { Dialog, Input } from "nsh/base";
import type { Accessor } from "nsh";

import type { AppStore } from "../app/composition/store";
import type { Setup, SetupKind } from "../app/domain/model";
import { summarizeSetup } from "../app/domain/setup";
import { dialogWidth } from "../app/presentation/layout";
import { centeredDialog } from "./ui";

type Tone = "ok" | "warn" | "danger" | "muted";

function summary(setup: Setup): string {
  const counts = summarizeSetup(setup);
  if (setup.kind === "fans") {
    return `${counts.manual} manual · ${counts.curves} curve · ${counts.headers} headers`;
  }
  const colour =
    counts.colors.length === 0
      ? "all off"
      : counts.colors.length === 1
        ? counts.colors[0].toUpperCase()
        : `${counts.colors.length} colours`;
  return `${counts.lit}/${counts.channels} lit · ${colour}`;
}

function stamp(ms: number): string {
  if (!ms) return "unknown date";
  return new Date(ms).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** A press target drawn with a View: buttons in a Dialog's popup on the
 *  desktops were not being hit by tests, and the app styles every control. */
function Action(props: {
  label: string;
  testID?: string;
  primary?: boolean;
  danger?: boolean;
  disabled?: boolean;
  onPress: () => void;
}) {
  const tone = props.disabled
    ? "border border-border opacity-50"
    : props.primary
      ? "bg-primary"
      : props.danger
        ? "border border-[#f87171]"
        : "border border-border";
  return (
    <View
      testID={props.testID}
      role="button"
      class={`rounded-[6px] px-2.5 py-1 ${tone}`}
      onPress={() => {
        if (!props.disabled) props.onPress();
      }}
    >
      <Text
        class={`text-[11px] ${
          props.primary
            ? "text-primary-foreground"
            : props.danger
              ? "text-[#f87171]"
              : "text-foreground"
        }`}
      >
        {props.label}
      </Text>
    </View>
  );
}

export function SetupsDialog(props: {
  kind: SetupKind;
  open: Accessor<boolean>;
  onOpenChange: (open: boolean) => void;
  store: AppStore;
  toast: (message: string, tone?: Tone) => void;
}) {
  const setups = signal<Setup[]>([]);
  const warning = signal<string | null>(null);
  const note = signal<string | null>(null);
  const name = signal("");
  const busy = signal<string | null>(null);
  const armed = signal<string | null>(null);

  const noun = () => (props.kind === "fans" ? "Fan presets" : "Lighting presets");
  const lead = () =>
    props.kind === "fans"
      ? "Fan settings only — lighting is left alone."
      : "Colour and brightness only — fans are left alone.";

  const load = async (): Promise<void> => {
    try {
      const list = await props.store.setups.list(props.kind);
      setups.set(list.setups);
      warning.set(list.warning);
    } catch (error) {
      note.set(`Could not load presets: ${String(error)}`);
    }
  };

  effect(() => {
    if (props.open()) void load();
  });

  const save = async (id: string | null): Promise<void> => {
    const target = id ? (setups().find((entry) => entry.id === id)?.name ?? "") : name();
    busy.set(id ?? "new");
    try {
      const list = await props.store.setups.save(props.kind, target, id ?? undefined);
      setups.set(list.setups);
      warning.set(list.warning);
      note.set(null);
      if (!id) name.set("");
      props.toast(id ? `“${target}” updated` : `“${target}” saved`, "ok");
    } catch (error) {
      note.set(`Could not save: ${String(error)}`);
    } finally {
      busy.set(null);
    }
  };

  const apply = async (setup: Setup): Promise<void> => {
    busy.set(setup.id);
    try {
      props.toast(await props.store.setups.apply(setup.id), "ok");
      props.onOpenChange(false);
    } catch (error) {
      note.set(`Could not apply: ${String(error)}`);
    } finally {
      busy.set(null);
    }
  };

  const remove = async (setup: Setup): Promise<void> => {
    busy.set(setup.id);
    try {
      const list = await props.store.setups.remove(props.kind, setup.id);
      setups.set(list.setups);
      warning.set(list.warning);
      props.toast(`“${setup.name}” deleted`, "ok");
    } catch (error) {
      note.set(`Could not delete: ${String(error)}`);
    } finally {
      busy.set(null);
      armed.set(null);
    }
  };

  const armDelete = (setup: Setup): void => {
    if (armed() === setup.id) {
      void remove(setup);
      return;
    }
    armed.set(setup.id);
    setTimeout(() => {
      if (armed() === setup.id) armed.set(null);
    }, 4000);
  };

  return (
    <Dialog
      open={props.open()}
      onOpenChange={({ open }) => props.onOpenChange(open)}
      accessibilityLabel={noun()}
      class="rounded-[12px] border border-border bg-popover p-4 gap-3"
      style={centeredDialog(dialogWidth(560))}
      backdropClass="bg-black/40"
    >
      <Text class="text-base font-semibold text-foreground">{noun()}</Text>
      <Text class="text-xs text-muted-foreground">{lead()}</Text>

      <Input
        value={name()}
        placeholder={
          props.kind === "fans"
            ? "Name this preset — e.g. Silent, Bench"
            : "Name this preset — e.g. Night, Amber"
        }
        accessibilityLabel="Preset name"
        onChange={({ value }) => name.set(value)}
        onSubmit={() => void save(null)}
        class="h-8 rounded-[6px] border border-border bg-muted px-2 text-xs text-foreground"
      />
      <Action
        testID={`setup-save-${props.kind}`}
        label="Save preset"
        primary
        disabled={busy() === "new"}
        onPress={() => void save(null)}
      />

      <If when={warning()}>
        {(message) => (
          <Text class="text-xs text-[#fbbf24]">{`Could not read the presets file: ${message()}`}</Text>
        )}
      </If>
      <If when={note()}>
        {(message) => <Text class="text-xs text-[#f87171]">{message()}</Text>}
      </If>

      <View class="gap-2">
        <Each of={setups()} key={(setup) => setup.id}>
          {(setup) => (
            <View class="rounded-[8px] border border-border bg-card p-2 gap-1">
              <Text class="text-xs font-semibold text-foreground">{setup().name}</Text>
              <Text class="text-[11px] text-muted-foreground">{summary(setup())}</Text>
              <Text class="text-[10px] font-mono text-muted-foreground">
                {stamp(setup().saved_at_ms)}
              </Text>
              <View class="flex-row items-center gap-1.5" style={{ flexWrap: "wrap" }}>
                <Action
                  testID={`setup-apply-${setup().id}`}
                  label={busy() === setup().id ? "Working…" : "Apply"}
                  primary
                  disabled={busy() === setup().id}
                  onPress={() => void apply(setup())}
                />
                <Action
                  label="Update"
                  disabled={busy() === setup().id}
                  onPress={() => void save(setup().id)}
                />
                <Action
                  label={armed() === setup().id ? "Confirm" : "Delete"}
                  danger={armed() === setup().id}
                  disabled={busy() === setup().id}
                  onPress={() => armDelete(setup())}
                />
              </View>
            </View>
          )}
        </Each>

        <If when={setups().length === 0}>
          <Text class="text-xs text-muted-foreground">
            Nothing saved yet. Set things the way you like them, then save the state to
            recall it later.
          </Text>
        </If>
      </View>

      <If when={armed()}>
        <Text class="text-[10px] text-muted-foreground">
          Press Delete again within four seconds to remove the preset.
        </Text>
      </If>

      <Action label="Close" onPress={() => props.onOpenChange(false)} />
    </Dialog>
  );
}
