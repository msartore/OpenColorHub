/**
 * Saved setups, embedded where the state lives.
 *
 * A setup spans both fans and lighting, so the same panel appears on the Fan
 * control and Lighting screens rather than on a screen of its own: from either
 * one you can save what is running, or apply a saved combination (which writes
 * both halves — that is what a setup is).
 */

import { api } from "../api";
import { el } from "../dom";
import type { Setup, SetupList } from "../types";
import type { ViewContext } from "../view";
import { button, emptyState, inlineNote, panel } from "./ui";

const hexOf = (channel: Setup["channels"][number]): string =>
  `#${channel.color.map((part) => part.toString(16).padStart(2, "0")).join("")}`;

function summarise(setup: Setup): string {
  const curves = setup.fans.filter((fan) => fan.curve !== null).length;
  const manual = setup.fans.length - curves;
  const lit = setup.channels.filter((channel) => channel.enabled);
  const colours = [...new Set(lit.map(hexOf))];
  const colourText =
    colours.length === 0
      ? "all channels off"
      : colours.length === 1
        ? colours[0].toUpperCase()
        : `${colours.length} colours`;
  return `${manual} manual · ${curves} curve · ${lit.length}/${setup.channels.length} lit · ${colourText}`;
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

export interface SetupsPanelOptions {
  /** Emphasised half, used in the subtitle so the panel reads as local. */
  focus: "fans" | "lighting";
}

/** Builds the panel; the caller appends `el` to its screen. */
export function setupsPanel(ctx: ViewContext, options: SetupsPanelOptions): { el: HTMLElement } {
  const root = el("div", { class: "setups-embed" });
  const body = el("div", { class: "setups-body" });
  const notes = el("div", { class: "setups-notes" });

  const nameInput = el("input", {
    class: "input setups-name",
    type: "text",
    placeholder: "Name this setup — e.g. Night, Bench, Silent",
    maxlength: 48,
    "aria-label": "Setup name",
  }) as HTMLInputElement;

  const saveButton = button("Save current", {
    variant: "primary",
    size: "sm",
    onClick: () => void save(null),
  });

  let setups: Setup[] = [];
  let busy: string | null = null;
  /** Row armed for deletion: the first click only warns. */
  let armed: string | null = null;

  function applyList(list: SetupList): void {
    setups = list.setups;
    notes.replaceChildren(
      list.warning ? inlineNote(`Could not read the setups file: ${list.warning}`, "danger") : "",
    );
    render();
  }

  async function refresh(): Promise<void> {
    try {
      applyList(await api.setups());
    } catch (error) {
      ctx.toast(`Could not load setups: ${String(error)}`, "danger");
    }
  }

  async function save(id: string | null): Promise<void> {
    const name = id
      ? setups.find((setup) => setup.id === id)?.name ?? ""
      : nameInput.value.trim();
    if (!name) {
      ctx.toast("Give the setup a name first", "warn");
      nameInput.focus();
      return;
    }
    busy = id ?? "new";
    render();
    try {
      const list = await api.saveSetup(name, id ?? undefined);
      applyList(list);
      if (!id) nameInput.value = "";
      ctx.toast(id ? `“${name}” updated` : `“${name}” saved`, "ok");
    } catch (error) {
      ctx.toast(`Could not save: ${String(error)}`, "danger");
    } finally {
      busy = null;
      render();
    }
  }

  async function apply(setup: Setup): Promise<void> {
    busy = setup.id;
    render();
    try {
      const result = await api.applySetup(setup.id);
      ctx.applySnapshot(result.snapshot);
      ctx.toast(result.message, "ok");
    } catch (error) {
      ctx.toast(`Could not apply: ${String(error)}`, "danger");
    } finally {
      busy = null;
      render();
    }
  }

  async function remove(setup: Setup): Promise<void> {
    busy = setup.id;
    render();
    try {
      applyList(await api.deleteSetup(setup.id));
      ctx.toast(`“${setup.name}” deleted`, "ok");
    } catch (error) {
      ctx.toast(`Could not delete: ${String(error)}`, "danger");
    } finally {
      busy = null;
      armed = null;
      render();
    }
  }

  function row(setup: Setup): HTMLElement {
    const working = busy === setup.id;
    const confirm = armed === setup.id;

    return el(
      "li",
      { class: "setup-row" },
      el(
        "div",
        { class: "setup-main" },
        el("div", { class: "setup-name" }, setup.name),
        el(
          "div",
          {
            class: "setup-summary",
            title: "Manual headers · curve-driven headers · lighting channels on · colour",
          },
          summarise(setup),
        ),
        el("div", { class: "setup-meta mono" }, stamp(setup.saved_at_ms)),
      ),
      el(
        "div",
        { class: "setup-actions" },
        button(working ? "Working…" : "Apply", {
          variant: "primary",
          size: "sm",
          disabled: working,
          title: "Write this setup — fans and lighting — to the hardware",
          onClick: () => void apply(setup),
        }),
        button("Update", {
          variant: "ghost",
          size: "sm",
          disabled: working,
          title: "Replace this setup with what is running now",
          onClick: () => void save(setup.id),
        }),
        button(confirm ? "Confirm delete" : "Delete", {
          variant: confirm ? "danger" : "ghost",
          size: "sm",
          disabled: working,
          onClick: () => {
            if (armed === setup.id) {
              void remove(setup);
            } else {
              armed = setup.id;
              render();
              window.setTimeout(() => {
                if (armed === setup.id) {
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
    body.replaceChildren();
    saveButton.disabled = busy === "new";

    if (setups.length === 0) {
      body.append(
        emptyState(
          "Nothing saved yet",
          "Set things the way you like them, then save the state to recall it later.",
        ),
      );
    } else {
      body.append(el("ul", { class: "setup-rows" }, ...setups.map(row)));
    }
  }

  const subtitle =
    options.focus === "fans"
      ? "Save or recall fan and lighting state together"
      : "Save or recall lighting and fan state together";

  root.append(
    panel({
      className: "setups-list",
      title: "Setups",
      subtitle,
      body: [
        notes,
        el("div", { class: "setups-form" }, nameInput, saveButton),
        body,
      ],
    }),
  );

  render();
  void refresh();

  nameInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") void save(null);
  });

  return { el: root };
}
