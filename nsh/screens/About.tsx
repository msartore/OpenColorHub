/** About: the detected board, build facts, paths and the upstream credit. */

import { Clipboard, Image, Text, View, openUrl, signal } from "nsh";
import { Button } from "nsh/base";

import type { AppStore } from "../app/composition/store";
import type { AboutInfo } from "../app/domain/model";
import { boardDisplayName } from "../app/domain/telemetry";
import logo from "../assets/icon.png";
import { Chip, KeyValue, Panel } from "../components/ui";

type Tone = "ok" | "warn" | "danger" | "muted";

function supportTone(confidence: string): Tone {
  if (confidence === "verified") return "ok";
  if (confidence === "unsupported") return "danger";
  if (confidence === "compatible") return "ok";
  return "warn";
}

export function About(props: {
  store: AppStore;
  toast: (message: string, tone?: Tone) => void;
}) {
  const about = signal<AboutInfo | null>(null);
  const error = signal<string | null>(null);

  const load = async (): Promise<void> => {
    try {
      about.set(await props.store.system.about());
      error.set(null);
    } catch (cause) {
      error.set(String(cause));
    }
  };
  void load();

  const copy = (value: string): void => {
    void Clipboard.writeText(value)
      .then(() => props.toast("Copied to clipboard", "ok"))
      .catch(() => props.toast("Clipboard unavailable", "warn"));
  };

  const openUpstream = (url: string): void => {
    void openUrl(url).catch((cause) => props.toast(String(cause), "danger"));
  };

  return (
    <View class="gap-4">
      <Text class="text-xl font-semibold text-foreground">About</Text>

      <Panel>
        <View class="flex-row items-center gap-4">
          <Image src={logo} style={{ width: 64, height: 64, borderRadius: 14 }} />
          <View class="gap-1">
            <Text class="text-lg font-semibold text-foreground">OpenColorHub</Text>
            <Text class="text-xs text-muted-foreground">
              Fan and lighting control for Colorful AMD motherboards.
            </Text>
            <View class="flex-row gap-1.5">
              <Chip label={`v${about()?.version ?? "—"}`} tone="muted" />
              <Chip label="GPL-3.0" tone="muted" />
              <Chip label="NSH" tone="ok" />
            </View>
          </View>
        </View>
      </Panel>

      <If when={error()}>
        {(message) => (
          <Panel title="About this build is unavailable">
            <Text class="text-xs text-muted-foreground">
              The build metadata could not be read, so the version, paths and licence are
              unknown for this install.
            </Text>
            <Text class="text-[11px] font-mono text-muted-foreground">{message()}</Text>
            <Button class="rounded-[7px] border border-border px-3 py-1.5" onPress={() => void load()}>
              <Text class="text-xs text-foreground">Try again</Text>
            </Button>
          </Panel>
        )}
      </If>

      <If when={about()}>
        {(info) => (
          <View class="gap-4">
            <Panel
              title="Detected motherboard"
              subtitle="Read from SMBIOS at startup — this is what the hardware guard checks"
              actions={
                <Chip
                  label={`${info().board.confidence} board`}
                  tone={supportTone(info().board.confidence)}
                />
              }
            >
              <If
                when={info().board.identity}
                else={
                  <Text class="text-xs text-muted-foreground">
                    SMBIOS did not report a motherboard model, so the hardware guard
                    cannot match this machine to the verified catalogue.
                  </Text>
                }
              >
                {(board) => (
                  <View class="flex-row gap-6" style={{ flexWrap: "wrap" }}>
                    <View class="gap-1" style={{ flex: 1 }}>
                      <KeyValue label="vendor" value={board().vendor || "—"} />
                      <KeyValue label="model" value={board().name || "—"} />
                      <KeyValue label="revision" value={board().version || "—"} />
                      <KeyValue label="system" value={board().system_product || "—"} />
                    </View>
                    <View class="gap-1" style={{ flex: 1 }}>
                      <KeyValue
                        label="bios"
                        value={
                          `${board().bios_vendor ?? ""} ${board().bios_version ?? ""}`.trim() ||
                          "—"
                        }
                      />
                      <KeyValue label="detected via" value={info().board.source} />
                      <KeyValue
                        label="series"
                        value={info().board.series ?? info().board.family ?? "—"}
                      />
                      <KeyValue
                        label="catalogue"
                        value={
                          info().board.matched ??
                          `${info().board.catalog_size} verified boards`
                        }
                      />
                    </View>
                  </View>
                )}
              </If>
              <Text class="text-xs text-muted-foreground">
                {`Board: ${boardDisplayName(info().board)}`}
              </Text>
              <Each of={info().board.notes} key="index">
                {(note) => <Text class="text-[11px] text-muted-foreground">{note()}</Text>}
              </Each>
            </Panel>

            <View class="flex-row gap-4" style={{ flexWrap: "wrap" }}>
              <View style={{ flex: 1 }}>
                <Panel title="Build" subtitle="What compiled this binary">
                  <View class="gap-1">
                    <KeyValue label="version" value={info().build.version} />
                    <KeyValue label="rustc" value={info().build.rustc} />
                    <KeyValue label="profile" value={info().build.profile} />
                    <KeyValue label="target" value={`${info().build.os} · ${info().build.arch}`} />
                    <KeyValue label="chip" value={info().chip} />
                    <KeyValue label="LEDs" value={String(info().total_leds)} />
                  </View>
                </Panel>
              </View>
              <View style={{ flex: 1 }}>
                <Panel title="Paths" subtitle="Where this install keeps its data">
                  <View class="gap-1">
                    <KeyValue label="data dir" value={info().paths.data_dir} />
                    <KeyValue label="settings" value={info().paths.settings} />
                    <KeyValue label="lighting state" value={info().paths.state} />
                  </View>
                  <Button
                    class="rounded-[7px] border border-border px-3 py-1.5"
                    onPress={() => copy(info().paths.data_dir)}
                  >
                    <Text class="text-xs text-foreground">Copy data dir</Text>
                  </Button>
                </Panel>
              </View>
            </View>

            <Panel title="Upstream" subtitle="The project this work derives from">
              <Text class="text-xs text-muted-foreground">
                {`Register programming and state format derived from ${info().upstream.name} by ${info().upstream.author}, ${info().upstream.license} licensed.`}
              </Text>
              <Button
                class="rounded-[7px] border border-border px-3 py-1.5"
                onPress={() => openUpstream(info().upstream.url)}
              >
                <Text class="text-xs text-foreground">{info().upstream.url}</Text>
              </Button>
            </Panel>
          </View>
        )}
      </If>
    </View>
  );
}
