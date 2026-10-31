/**
 * Empty and error states.
 *
 * A blank panel cannot tell the difference between "nothing here yet" and
 * "something went wrong", so both states use the same shape — a large icon, a
 * title, a sentence saying what happened — and only the icon and tone differ.
 */

import { el } from "../dom";
import { icon, type IconName } from "./icons";

export interface StateAction {
  label: string;
  onClick: () => void;
  variant?: "primary" | "ghost" | "quiet" | "danger";
}

export interface StateOptions {
  kind: "empty" | "error";
  icon: IconName;
  title: string;
  description: string;
  /** One line of technical detail, shown in mono under the description. */
  detail?: string | null;
  actions?: StateAction[];
  /** Shrinks the icon and spacing for use inside a panel rather than a screen. */
  compact?: boolean;
}

/** A container that swaps between a state and the content it replaces. */
export interface StateSlot {
  el: HTMLElement;
  show(options: StateOptions): void;
  clear(): void;
}

export function stateSlot(): StateSlot {
  const host = el("div", { class: "state-host" });
  host.hidden = true;
  return {
    el: host,
    show(options) {
      host.replaceChildren(stateView(options));
      host.hidden = false;
    },
    clear() {
      host.replaceChildren();
      host.hidden = true;
    },
  };
}

export function stateView(options: StateOptions): HTMLElement {
  const actions = (options.actions ?? []).map((action) =>
    el(
      "button",
      {
        class: `btn btn--${action.variant ?? "ghost"} btn--sm`,
        type: "button",
        onClick: action.onClick,
      },
      action.label,
    ),
  );

  return el(
    "div",
    { class: `state state--${options.kind}${options.compact ? " state--compact" : ""}`, role: options.kind === "error" ? "alert" : "status" },
    el("div", { class: "state-icon" }, icon(options.icon)),
    el("p", { class: "state-title" }, options.title),
    el("p", { class: "state-description" }, options.description),
    options.detail ? el("p", { class: "state-detail mono" }, options.detail) : null,
    actions.length ? el("div", { class: "state-actions" }, ...actions) : null,
  );
}
