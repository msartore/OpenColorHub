/**
 * Saved presets, behind a save icon.
 *
 * The list was a panel on the screen, which put a save form and a row of
 * buttons in front of the controls people actually came for. It is now one icon
 * button in the screen header that opens a dialog: the presets are there when
 * you want them and gone when you do not.
 *
 * A preset covers one half — fans or lighting — so each screen opens its own
 * list, and applying one writes only that half.
 */

import { api } from "../api";
import { el } from "../dom";
import { icon } from "./icons";
import { button, emptyState, inlineNote } from "./ui";
import type { Setup, SetupKind, SetupList } from "../types";
import type { ViewContext } from "../view";

const hexOf = (channel: Setup["channels"][number]): string =>
  `#${channel.color.map((part) => part.toString(16).padStart(2, "0")).join("")}`;

function summarise(setup: Setup): string {
  if (setup.kind === "fans") {
    const curves = setup.fans.filter((fan) => fan.curve !== null).length;
    const manual = setup.fans.length - curves;
    return `${manual} manual · ${curves} curve · ${setup.fans.length} headers`;
  }
  const lit = setup.channels.filter((channel) => channel.enabled);
  const colours = [...new Set(lit.map(hexOf))];
  const colour = colours.length === 0 ? "all off" : colours.length === 1 ? colours[0].toUpperCase() : `${colours.length} colours`;
  return `${lit.length}/${setup.channels.length} lit · ${colour}`;
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

export interface PresetsButtonOptions {
  /** Which half this screen saves and applies. */
  kind: SetupKind;
}

/** Builds the header icon button; clicking it opens the presets dialog. */
export function presetsButton(ctx: ViewContext, options: PresetsButtonOptions): { el: HTMLButtonElement } {
  const trigger = el(
    "button",
    {
      class: "icon-btn",
      type: "button",
      title: options.kind === "fans" ? "Fan presets" : "Lighting presets",
      "aria-label": options.kind === "fans" ? "Fan presets" : "Lighting presets",
    },
    icon("save", "icon-btn-glyph"),
  ) as HTMLButtonElement;

  trigger.addEventListener("click", () => openDialog(ctx, options.kind));
  return { el: trigger };
}

function openDialog(ctx: ViewContext, kind: SetupKind): void {
  const noun = kind === "fans" ? "Fan presets" : "Lighting presets";
  const lead =
    kind === "fans"
      ? "Fan settings only — lighting is left alone."
      : "Colour and brightness only — fans are left alone.";

  const list = el("div", { class: "preset-list" });
  const note = el("div", {});
  const nameInput = el("input", {
    class: "input preset-name",
    type: "text",
    placeholder: kind === "fans" ? "Name this preset — e.g. Silent, Bench" : "Name this preset — e.g. Night, Amber",
    maxlength: 48,
    "aria-label": "Preset name",
  }) as HTMLInputElement;

  const saveButton = button(kind === "fans" ? "Save fan preset" : "Save lighting preset", {
    variant: "primary",
    onClick: () => void save(null),
  });

  const closeButton = button("Close", { variant: "quiet", onClick: () => close() });

  let presets: Setup[] = [];
  let busy: string | null = null;
  /** Row armed for deletion: the first click only warns. */
  let armed: string | null = null;

  const closeIcon = el(
    "button",
    { class: "icon-btn dialog-close", type: "button", title: "Close", "aria-label": "Close" },
    "×",
  );
  closeIcon.addEventListener("click", () => close());

  const card = el(
    "div",
    { class: "update-card presets-card" },
    el("div", { class: "update-head" }, el("h2", { class: "update-title" }, noun), closeIcon),
    el("p", { class: "update-lead" }, lead),
    note,
    list,
    el("div", { class: "preset-form" }, nameInput, saveButton),
    el("div", { class: "update-actions" }, closeButton),
  );

  const backdrop = el("div", { class: "update-backdrop", role: "dialog", "aria-modal": "true" }, card);

  function close(): void {
    backdrop.remove();
    document.removeEventListener("keydown", onKey);
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "Escape") close();
  }

  function applyList(next: SetupList): void {
    presets = next.setups;
    note.replaceChildren(next.warning ? inlineNote(`Could not read the presets file: ${next.warning}`, "danger") : "");
    render();
  }

  async function refresh(): Promise<void> {
    try {
      applyList(await api.setups(kind));
    } catch (error) {
      note.replaceChildren(inlineNote(`Could not load presets: ${String(error)}`, "danger"));
    }
  }

  async function save(id: string | null): Promise<void> {
    const name = id ? (presets.find((preset) => preset.id === id)?.name ?? "") : nameInput.value.trim();
    if (!name) {
      note.replaceChildren(inlineNote("Give the preset a name first.", "warn"));
      nameInput.focus();
      return;
    }
    busy = id ?? "new";
    render();
    try {
      applyList(await api.saveSetup(kind, name, id ?? undefined));
      if (!id) nameInput.value = "";
      note.replaceChildren();
      ctx.toast(id ? `“${name}” updated` : `“${name}” saved`, "ok");
    } catch (error) {
      note.replaceChildren(inlineNote(`Could not save: ${String(error)}`, "danger"));
    } finally {
      busy = null;
      render();
    }
  }

  async function apply(preset: Setup): Promise<void> {
    busy = preset.id;
    render();
    try {
      const result = await api.applySetup(preset.id);
      ctx.applySnapshot(result.snapshot);
      ctx.toast(result.message, "ok");
      close();
    } catch (error) {
      note.replaceChildren(inlineNote(`Could not apply: ${String(error)}`, "danger"));
    } finally {
      busy = null;
      render();
    }
  }

  async function remove(preset: Setup): Promise<void> {
    busy = preset.id;
    render();
    try {
      applyList(await api.deleteSetup(kind, preset.id));
      ctx.toast(`“${preset.name}” deleted`, "ok");
    } catch (error) {
      note.replaceChildren(inlineNote(`Could not delete: ${String(error)}`, "danger"));
    } finally {
      busy = null;
      armed = null;
      render();
    }
  }

  function row(preset: Setup): HTMLElement {
    const working = busy === preset.id;
    const confirm = armed === preset.id;

    return el(
      "div",
      { class: "preset-item" },
      el(
        "div",
        { class: "preset-main" },
        el("div", { class: "preset-title" }, preset.name),
        el("div", { class: "preset-summary" }, summarise(preset)),
        el("div", { class: "preset-date mono" }, stamp(preset.saved_at_ms)),
      ),
      el(
        "div",
        { class: "preset-actions" },
        button(working ? "Working…" : "Apply", {
          variant: "primary",
          size: "sm",
          disabled: working,
          onClick: () => void apply(preset),
        }),
        button("Update", {
          variant: "ghost",
          size: "sm",
          disabled: working,
          title: "Replace this preset with what is running now",
          onClick: () => void save(preset.id),
        }),
        button(confirm ? "Confirm" : "Delete", {
          variant: confirm ? "danger" : "ghost",
          size: "sm",
          disabled: working,
          onClick: () => {
            if (armed === preset.id) {
              void remove(preset);
            } else {
              armed = preset.id;
              render();
              window.setTimeout(() => {
                if (armed === preset.id) {
                  armed = null;
                  render();
                }
              }, 4000);
            }
          },
        }),
      ),
    );
  }

  function render(): void {
    list.replaceChildren();
    saveButton.disabled = busy === "new";

    if (presets.length === 0) {
      list.append(
        emptyState(
          "Nothing saved yet",
          "Set things the way you like them, then save the state to recall it later.",
        ),
      );
      return;
    }
    list.append(...presets.map(row));
  }

  nameInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") void save(null);
  });
  backdrop.addEventListener("click", (event) => {
    if (event.target === backdrop) close();
  });
  document.addEventListener("keydown", onKey);

  document.body.appendChild(backdrop);
  render();
  void refresh();
  nameInput.focus();
}
