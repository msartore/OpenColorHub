/** Drivers: the vendor's own downloads for this board, fetched from Colorful. */

import { api, onDriverProgress } from "../api";
import { el } from "../dom";
import type { DriverList, DriverProgress, DriverRow, DownloadedFile, Snapshot } from "../types";
import type { View, ViewContext } from "../view";
import { badge, button, inlineNote, panel } from "../components/ui";
import { stateView } from "../components/state";

const KIND_LABELS: Record<DriverRow["kind"], string> = {
  bios: "Firmware",
  driver: "Drivers",
  utility: "Utilities",
  manual: "Manuals",
  other: "Other",
};

/** Firmware first, then drivers, then the rest — mirrors the core's ordering. */
const ORDER: DriverRow["kind"][] = ["bios", "driver", "utility", "manual", "other"];

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

interface ProgressNodes {
  wrap: HTMLElement;
  fill: HTMLElement;
  label: HTMLElement;
}

export function createDrivers(ctx: ViewContext): View {
  const root = el("div", { class: "screen" });
  const actions = el("div", { class: "screen-actions" });
  const head = el(
    "div",
    { class: "screen-head" },
    el(
      "div",
      {},
      el("h1", { class: "screen-title" }, "Drivers"),
      el(
        "p",
        { class: "screen-sub" },
        "Files Colorful publishes for this board. Downloads are saved to the app's data folder — nothing is run for you.",
      ),
    ),
    actions,
  );
  const status = el("p", { class: "screen-sub" }, "Looking up the board…");
  const body = el("div", { class: "drivers-stack" });

  actions.append(button("Refresh", { variant: "ghost", onClick: () => void load(true) }));

  let catalogue: DriverList | null = null;
  let error: string | null = null;
  let loading = true;
  /** Latest progress per file name. */
  const progress = new Map<string, DriverProgress>();
  /** Live DOM refs, so a chunk updates its own row instead of re-rendering. */
  const progressNodes = new Map<string, ProgressNodes>();
  const rowButtons = new Map<string, HTMLButtonElement>();
  /** Files downloading right now; several may run at once, none blocks another. */
  const active = new Set<string>();
  /** Completed downloads, keyed by url, kept for the session. */
  const done = new Map<string, DownloadedFile>();

  function paintProgress(name: string, p: DriverProgress): void {
    const nodes = progressNodes.get(name);
    if (!nodes) return;
    nodes.wrap.classList.remove("is-hidden");
    if (p.total) {
      const pct = Math.min(100, Math.round((p.received / p.total) * 100));
      nodes.fill.style.width = `${pct}%`;
      nodes.fill.classList.remove("bar-fill--indeterminate");
      nodes.label.textContent = `${formatBytes(p.received)} of ${formatBytes(p.total)}`;
    } else {
      // No Content-Length: show motion, never a fake percentage.
      nodes.fill.style.width = "35%";
      nodes.fill.classList.add("bar-fill--indeterminate");
      nodes.label.textContent = `${formatBytes(p.received)} received`;
    }
  }

  let refreshing = false;

  /**
   * Shows the cached catalogue first and only then asks the vendor, so the
   * screen has content offline and updates in place when the network answers.
   */
  async function load(refresh = false, background = false): Promise<void> {
    if (background) {
      refreshing = true;
      render();
    } else {
      loading = true;
      error = null;
      progress.clear();
      render();
    }
    try {
      const list = await api.drivers(refresh);
      catalogue = list;
      error = null;
    } catch (e) {
      if (!background) {
        error = String(e);
        catalogue = null;
      }
    } finally {
      loading = false;
      refreshing = false;
      render();
    }
  }

  async function startDownload(row: DriverRow): Promise<void> {
    const name = row.file_name;
    active.add(row.url);
    const initial: DriverProgress = { file: name, received: 0, total: null };
    progress.set(name, initial);
    paintProgress(name, initial);

    const btn = rowButtons.get(name);
    if (btn) {
      btn.textContent = "Downloading…";
      btn.disabled = true;
    }

    try {
      const saved = await api.downloadDriver(row);
      done.set(row.url, saved);
      ctx.toast(`${name} saved`, "ok");
    } catch (e) {
      ctx.toast(`Download failed: ${String(e)}`, "danger");
    } finally {
      active.delete(row.url);
      progress.delete(name);
      progressNodes.get(name)?.wrap.classList.add("is-hidden");
      render();
    }
  }

  function rowElement(row: DriverRow): HTMLElement {
    const saved = done.get(row.url);
    const name = row.file_name;

    const fill = el("div", { class: "bar-fill" });
    const label = el("div", { class: "table-sub" });
    const wrap = el(
      "div",
      { class: "driver-progress-wrap is-hidden" },
      el("div", { class: "bar bar--thin driver-progress" }, fill),
      label,
    );
    progressNodes.set(name, { wrap, fill, label });
    const current = progress.get(name);
    if (current) paintProgress(name, current);

    let control: HTMLElement;
    if (saved) {
      control = button("Show in folder", {
        variant: "ghost",
        onClick: () =>
          void api.revealDriver(saved.path).catch((e) => ctx.toast(String(e), "danger")),
      });
    } else {
      const downloading = active.has(row.url);
      const btn = button(downloading ? "Downloading…" : "Download", {
        variant: "primary",
        disabled: downloading,
        onClick: () => void startDownload(row),
      });
      rowButtons.set(name, btn);
      control = btn;
    }

    return el(
      "tr",
      {},
      el(
        "td",
        {},
        el(
          "div",
          { class: "driver-title" },
          row.title,
          row.newer ? badge("newer", "warn") : null,
        ),
        el(
          "div",
          { class: "driver-detail" },
          el("div", { class: "driver-file" }, name),
          el(
            "div",
            { class: "table-sub" },
            [row.published || "no date", saved ? formatBytes(saved.bytes) : null]
              .filter(Boolean)
              .join(" · "),
          ),
          wrap,
        ),
      ),
      el("td", { class: "driver-actions" }, control),
    );
  }

  /** Firmware releases pile up; show the newest few and offer the rest. */
  const VISIBLE_FIRMWARE = 4;
  const expanded = new Set<DriverRow["kind"]>();

  function group(kind: DriverRow["kind"], rows: DriverRow[]): HTMLElement | null {
    if (rows.length === 0) return null;

    const collapsible = kind === "bios" && rows.length > VISIBLE_FIRMWARE && !expanded.has(kind);
    const shown = collapsible ? rows.slice(0, VISIBLE_FIRMWARE) : rows;

    const body = el(
      "table",
      { class: "data-table" },
      el("thead", {}, el("tr", {}, el("th", {}, KIND_LABELS[kind]), el("th", {}, ""))),
      el("tbody", {}, ...shown.map(rowElement)),
    );

    if (collapsible) {
      body.append(
        el(
          "tbody",
          {},
          el(
            "tr",
            {},
            el(
              "td",
              { colspan: "2", class: "driver-more" },
              button(`Show all ${rows.length} firmware releases`, {
                variant: "quiet",
                size: "sm",
                onClick: () => {
                  expanded.add(kind);
                  render();
                },
              }),
            ),
          ),
        ),
      );
    }

    return panel({
      className: "drivers-panel",
      title: KIND_LABELS[kind],
      subtitle: `${rows.length} file${rows.length === 1 ? "" : "s"}`,
      body: [body],
    });
  }

  function render(): void {
    body.replaceChildren();
    rowButtons.clear();
    progressNodes.clear();

    if (loading) {
      status.textContent = "Looking up the board…";
      body.append(el("div", { class: "drivers-loading" }, "Fetching the vendor catalogue…"));
      return;
    }
    if (error) {
      status.textContent = "";
      body.append(
        stateView({
          kind: "error",
          icon: "offline",
          title: "Could not reach Colorful",
          description:
            "The download catalogue lives on colorfulgroup.com. Without it there is nothing to list for this board.",
          detail: error,
          actions: [{ label: "Try again", variant: "primary", onClick: () => void load(true) }],
        }),
      );
      return;
    }
    if (!catalogue) return;

    const { board, installed_bios: bios, files } = catalogue;
    status.replaceChildren(
      el("span", {}, board),
      el("span", { class: "screen-sub-sep" }, " · "),
      el("span", {}, bios ? `installed BIOS ${bios}` : "BIOS version unknown"),
      el("span", { class: "screen-sub-sep" }, " · "),
      el("span", {}, `${files.length} files`),
    );

    if (catalogue.fetched_at_ms) {
      const when = new Date(catalogue.fetched_at_ms).toLocaleString(undefined, {
        year: "numeric",
        month: "short",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
      });
      status.append(
        el("span", { class: "screen-sub-sep" }, " · "),
        el("span", { class: refreshing ? "is-stale" : "" }, `${refreshing ? "refreshing" : "as of"} ${when}`),
      );
    }

    if (catalogue.warning) {
      body.append(inlineNote(`Could not reach the vendor: ${catalogue.warning} — showing the cached list.`, "warn"));
    }

    const newer = files.filter((f) => f.newer);
    if (newer.length > 0) {
      body.append(
        inlineNote(
          `Firmware update available: ${newer.map((f) => f.title).join(", ")}. Flashing is done from the BIOS, not from here.`,
          "warn",
        ),
      );
    }

    for (const kind of ORDER) {
      const node = group(kind, files.filter((f) => f.kind === kind));
      if (node) body.append(node);
    }
    if (files.length === 0) {
      body.append(
        stateView({
          kind: "empty",
          icon: "inbox",
          title: "Nothing published for this board",
          description:
            "The vendor's catalogue lists no BIOS, driver or manual for this model. That is what Colorful publishes, not a failed lookup.",
        }),
      );
    }
  }

  root.append(head, status, body);
  render();
  void load().then(() => {
    // A cached answer is rendered immediately; the network copy replaces it.
    if (catalogue?.cached) void load(true, true);
  });

  void onDriverProgress((p) => {
    progress.set(p.file, p);
    paintProgress(p.file, p);
  }).catch(() => {
    /* progress is cosmetic; a missing listener must not break the view */
  });

  return {
    el: root,
    update(_snapshot: Snapshot): void {
      // The catalogue is independent of telemetry; nothing to redraw per tick.
    },
  };
}
