/**
 * Saved presets, embedded where the state lives.
 *
 * A preset covers one half — fans or lighting — so the Fan control screen hosts
 * the fan list and the Lighting screen hosts the lighting list. Keeping them
 * apart means recalling a fan curve cannot change the colour, and picking a
 * colour cannot change the fans.
 */

import { api } from "../api";
import { el } from "../dom";
import type { Setup, SetupKind, SetupList } from "../types";
import type { ViewContext } from "../view";
import { button, emptyState, inlineNote, panel } from "./ui";

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

export interface SetupsPanelOptions {
  /** Which half this panel saves and applies. */
  kind: SetupKind;
}

/** Builds the panel; the caller appends `el` to its screen. */
export function setupsPanel(ctx: ViewContext, options: SetupsPanelOptions): { el: HTMLElement } {
  const root = el("div", { class: "setups-embed" });
  const body = el("div", { class: "setups-body" });
  const notes = el("div", { class: "setups-notes" });

  const nameInput = el("input", {
    class: "input setups-name",
    type: "text",
    placeholder:
      options.kind === "fans"
        ? "Name this preset — e.g. Silent, Bench"
        : "Name this preset — e.g. Night, Amber",
    maxlength: 48,
    "aria-label": "Setup name",
  }) as HTMLInputElement;

  const saveButton = button(options.kind === "fans" ? "Save fan preset" : "Save lighting preset", {
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
      applyList(await api.setups(options.kind));
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
      const list = await api.saveSetup(options.kind, name, id ?? undefined);
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
      applyList(await api.deleteSetup(options.kind, setup.id));
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
            title:
            setup.kind === "fans"
              ? "Manual headers · curve-driven headers · total headers"
              : "Channels lit · colour",
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
    options.kind === "fans"
      ? "Fan settings only — lighting is left alone"
      : "Colour and brightness only — fans are left alone";

  root.append(
    panel({
      className: "setups-list",
      title: options.kind === "fans" ? "Fan presets" : "Lighting presets",
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
