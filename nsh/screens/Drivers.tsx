/** Drivers: the vendor catalogue, cached first, with downloads and progress. */

import { Text, View, computed, signal } from "nsh";
import { Button, Progress } from "nsh/base";

import type { AppStore } from "../app/composition/store";
import type {
  DownloadedFile,
  DriverKind,
  DriverList,
  DriverProgress,
  DriverRow,
  StoredFile,
} from "../app/domain/model";
import {
  DRIVER_KIND_LABELS,
  DRIVER_KIND_ORDER,
  newerDrivers,
  progressPercent,
  rowsOfKind,
  visibleRows,
} from "../app/domain/drivers";
import { backgroundRefresh } from "../app/application/usecases/drivers";
import { Chip, Notice, Panel } from "../components/ui";

type Tone = "ok" | "warn" | "danger" | "muted";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`;
}

function stamp(ms: number): string {
  if (!ms) return "never";
  return new Date(ms).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function Drivers(props: {
  store: AppStore;
  toast: (message: string, tone?: Tone) => void;
}) {
  const catalogue = signal<DriverList | null>(null);
  const loading = signal(true);
  const refreshing = signal(false);
  const error = signal<string | null>(null);
  const progress = signal<Record<string, DriverProgress>>({});
  const active = signal<string[]>([]);
  const downloaded = signal<Record<string, DownloadedFile>>({});
  const stored = signal<StoredFile[]>([]);
  const expanded = signal<DriverKind[]>([]);

  const groups = computed(() => {
    const files = catalogue()?.files ?? [];
    return DRIVER_KIND_ORDER.map((kind) => ({ kind, rows: rowsOfKind(files, kind) })).filter(
      (group) => group.rows.length > 0,
    );
  });

  const newer = computed(() => newerDrivers(catalogue()?.files ?? []));

  const load = async (refresh = false, background = false): Promise<void> => {
    if (background) refreshing.set(true);
    else {
      loading.set(true);
      error.set(null);
    }
    try {
      const list = await props.store.drivers.list(refresh);
      catalogue.set(list);
      error.set(null);
    } catch (cause) {
      if (!background) {
        error.set(String(cause));
        catalogue.set(null);
      }
    } finally {
      loading.set(false);
      refreshing.set(false);
    }
  };

  void load().then(() => {
    const list = catalogue();
    if (list && backgroundRefresh(list)) void load(true, true);
  });

  void props.store.drivers
    .stored()
    .then((files) => {
      stored.set(files);
      const known: Record<string, DownloadedFile> = { ...downloaded() };
      for (const file of files) {
        if (!known[file.file_name]) {
          known[file.file_name] = { path: file.path, bytes: file.bytes, sha256: "" };
        }
      }
      downloaded.set(known);
    })
    .catch(() => {
      // The download folder is optional; an empty list is fine.
    });

  const firstStored = (name: string): DownloadedFile | undefined => {
    const exact = downloaded()[name];
    if (exact) return exact;
    const fallback = stored().find((file) => file.file_name === name);
    return fallback
      ? { path: fallback.path, bytes: fallback.bytes, sha256: "" }
      : undefined;
  };

  const startDownload = async (row: DriverRow): Promise<void> => {
    active.set([...active(), row.url]);
    progress.set({
      ...progress(),
      [row.file_name]: { file: row.file_name, received: 0, total: null },
    });
    try {
      const saved = await props.store.drivers.download(row, (next) => {
        progress.set({ ...progress(), [next.file]: next });
      });
      downloaded.set({ ...downloaded(), [row.file_name]: saved });
      props.toast(`${row.file_name} saved`, "ok");
    } catch (cause) {
      props.toast(`Download failed: ${String(cause)}`, "danger");
    } finally {
      active.set(active().filter((url) => url !== row.url));
      const next = { ...progress() };
      delete next[row.file_name];
      progress.set(next);
    }
  };

  const show = (path: string): void => {
    void props.store.drivers
      .reveal(path)
      .catch((cause) => props.toast(String(cause), "danger"));
  };

  const toggleKind = (kind: DriverKind): void => {
    expanded.set(
      expanded().includes(kind)
        ? expanded().filter((entry) => entry !== kind)
        : [...expanded(), kind],
    );
  };

  const row = (file: DriverRow) => {
    const saved = firstStored(file.file_name);
    const current = progress()[file.file_name];
    const downloading = active().includes(file.url);

    return (
      <View class="flex-row items-start justify-between gap-3 border-b border-border py-2">
        <View class="gap-0.5" style={{ flexShrink: 1 }}>
          <View class="flex-row items-center gap-1.5">
            <Text class="text-xs font-semibold text-foreground">{file.title}</Text>
            <If when={file.newer}>
              <Chip label="newer" tone="warn" />
            </If>
          </View>
          <Text class="text-[11px] font-mono text-muted-foreground">{file.file_name}</Text>
          <Text class="text-[11px] text-muted-foreground">
            {[file.published || "no date", saved ? formatBytes(saved.bytes) : null]
              .filter(Boolean)
              .join(" · ")}
          </Text>
          <If when={current}>
            {(frame) => (
              <View class="gap-1" style={{ width: 220 }}>
                <Progress
                  value={progressPercent(frame().received, frame().total) ?? 0}
                  class="h-1"
                  trackClass="h-1 rounded-full bg-muted"
                  indicatorClass="h-1 rounded-full bg-primary"
                />
                <Text class="text-[10px] text-muted-foreground">
                  {frame().total
                    ? `${formatBytes(frame().received)} of ${formatBytes(frame().total!)}`
                    : `${formatBytes(frame().received)} received`}
                </Text>
              </View>
            )}
          </If>
        </View>
        <If
          when={saved}
          else={
            <Button
              testID={`driver-download-${file.file_name}`}
              class="rounded-[6px] bg-primary px-2.5 py-1"
              disabled={downloading}
              onPress={() => void startDownload(file)}
            >
              <Text class="text-[11px] text-primary-foreground">
                {downloading ? "Downloading…" : "Download"}
              </Text>
            </Button>
          }
        >
          {(entry) => (
            <Button
              testID={`driver-show-${file.file_name}`}
              class="rounded-[6px] border border-border px-2.5 py-1"
              onPress={() => show(entry().path)}
            >
              <Text class="text-[11px] text-foreground">Show in folder</Text>
            </Button>
          )}
        </If>
      </View>
    );
  };

  return (
    <View class="gap-4">
      <View class="flex-row items-center justify-between gap-3">
        <View class="gap-1">
          <Text class="text-xl font-semibold text-foreground">Drivers</Text>
          <Text class="text-xs text-muted-foreground">
            Files Colorful publishes for this board. Downloads are saved to the app's
            data folder — nothing is run for you.
          </Text>
        </View>
        <Button
          testID="driver-refresh"
          class="rounded-[7px] border border-border px-3 py-1.5"
          disabled={loading()}
          onPress={() => void load(true)}
        >
          <Text class="text-xs text-foreground">{refreshing() ? "Refreshing…" : "Refresh"}</Text>
        </Button>
      </View>

      <If when={catalogue()}>
        {(list) => (
          <View class="flex-row items-center gap-1.5" style={{ flexWrap: "wrap" }}>
            <Chip label={list().board} tone="muted" />
            <Chip
              label={
                list().installed_bios
                  ? `installed BIOS ${list().installed_bios}`
                  : "BIOS version unknown"
              }
              tone="muted"
            />
            <Chip label={`${list().files.length} files`} tone="muted" />
            <Chip
              label={`${refreshing() ? "refreshing" : "as of"} ${stamp(list().fetched_at_ms)}`}
              tone="muted"
            />
          </View>
        )}
      </If>

      <If when={loading()}>
        <Panel title="Fetching the vendor catalogue">
          <Text class="text-xs text-muted-foreground">Looking up the board…</Text>
        </Panel>
      </If>

      <If when={error()}>
        {(message) => (
          <Panel title="Could not reach Colorful">
            <Text class="text-xs text-muted-foreground">
              The download catalogue lives on colorfulgroup.com. Without it there is
              nothing to list for this board.
            </Text>
            <Text class="text-[11px] font-mono text-muted-foreground">{message()}</Text>
            <Button
              class="rounded-[7px] border border-border px-3 py-1.5"
              onPress={() => void load(true)}
            >
              <Text class="text-xs text-foreground">Try again</Text>
            </Button>
          </Panel>
        )}
      </If>

      <If when={catalogue()?.warning}>
        {(message) => (
          <Notice tone="warn" title="Showing the cached list">
            <Text class="text-xs text-muted-foreground">
              {`Could not reach the vendor: ${message()}`}
            </Text>
          </Notice>
        )}
      </If>

      <If when={newer().length > 0}>
        <Notice tone="warn" title="Firmware update available">
          <Text class="text-xs text-muted-foreground">
            {`${newer()
              .map((file) => file.title)
              .join(", ")}. Flashing is done from the BIOS, not from here.`}
          </Text>
        </Notice>
      </If>

      <Each of={groups()} key={(group) => group.kind}>
        {(group) => {
          const rows = visibleRows(group().kind, group().rows, expanded().includes(group().kind));
          return (
            <Panel
              title={DRIVER_KIND_LABELS[group().kind]}
              subtitle={`${group().rows.length} file${group().rows.length === 1 ? "" : "s"}`}
            >
              <Each of={rows.shown} key={(file) => file.file_name}>
                {(file) => row(file())}
              </Each>
              <If when={rows.collapsed}>
                <Button
                  class="rounded-[7px] border border-border px-3 py-1.5"
                  onPress={() => toggleKind(group().kind)}
                >
                  <Text class="text-xs text-foreground">
                    {`Show all ${group().rows.length} firmware releases`}
                  </Text>
                </Button>
              </If>
              <If when={!rows.collapsed && expanded().includes(group().kind) && group().kind === "bios"}>
                <Button
                  class="rounded-[7px] border border-border px-3 py-1.5"
                  onPress={() => toggleKind(group().kind)}
                >
                  <Text class="text-xs text-foreground">Show newest</Text>
                </Button>
              </If>
            </Panel>
          );
        }}
      </Each>

      <If when={!loading() && !error() && (catalogue()?.files.length ?? 0) === 0}>
        <Panel title="Nothing published for this board">
          <Text class="text-xs text-muted-foreground">
            The vendor's catalogue lists no BIOS, driver or manual for this model. That is
            what Colorful publishes, not a failed lookup.
          </Text>
        </Panel>
      </If>
    </View>
  );
}
