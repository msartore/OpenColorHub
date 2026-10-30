/** Reusable UI primitives. Every widget keeps its own DOM so views can update
 *  values on each telemetry tick without re-creating inputs (which would steal
 *  focus mid-drag). */

import { clamp, el } from "../dom";

export type Tone = "ok" | "warn" | "danger" | "muted" | "accent";

export interface PanelOptions {
  title?: string;
  subtitle?: string;
  actions?: HTMLElement[];
  body?: (Node | string)[];
  className?: string;
  flush?: boolean;
}

export function panel(options: PanelOptions): HTMLElement {
  const head =
    options.title || options.actions
      ? el(
          "header",
          { class: "panel-head" },
          el(
            "div",
            { class: "panel-titles" },
            options.title ? el("h2", { class: "panel-title" }, options.title) : null,
            options.subtitle ? el("p", { class: "panel-subtitle" }, options.subtitle) : null,
          ),
          options.actions ? el("div", { class: "panel-actions" }, ...options.actions) : null,
        )
      : null;

  return el(
    "section",
    { class: `panel${options.className ? ` ${options.className}` : ""}` },
    head,
    el(
      "div",
      { class: `panel-body${options.flush ? " panel-body--flush" : ""}` },
      ...(options.body ?? []),
    ),
  );
}

export function pill(text: string, tone: Tone = "muted"): HTMLElement {
  return el("span", { class: `pill pill--${tone}` }, el("span", { class: "pill-dot" }), text);
}

export function badge(text: string, tone: Tone = "muted"): HTMLElement {
  return el("span", { class: `badge badge--${tone}` }, text);
}

export interface ButtonOptions {
  variant?: "primary" | "ghost" | "danger" | "quiet";
  size?: "sm" | "md";
  disabled?: boolean;
  title?: string;
  onClick?: (event: MouseEvent) => void;
}

export function button(label: string, options: ButtonOptions = {}): HTMLButtonElement {
  const node = el(
    "button",
    {
      class: `btn btn--${options.variant ?? "ghost"}${options.size === "sm" ? " btn--sm" : ""}`,
      type: "button",
      title: options.title,
      disabled: options.disabled ?? false,
      onclick: options.onClick,
    },
    label,
  );
  return node;
}

export interface StatTileOptions {
  label: string;
  value: string;
  unit?: string;
  hint?: string;
  tone?: Tone;
}

export interface StatTile {
  el: HTMLElement;
  set(value: string, hint?: string, tone?: Tone): void;
}

export function statTile(options: StatTileOptions): StatTile {
  const valueNode = el("span", { class: "stat-value" }, options.value);
  const unitNode = options.unit ? el("span", { class: "stat-unit" }, options.unit) : null;
  const hintNode = el("span", { class: "stat-hint" }, options.hint ?? "");
  const root = el(
    "div",
    { class: `stat stat--${options.tone ?? "accent"}` },
    el("span", { class: "stat-label" }, options.label),
    el("span", { class: "stat-readout" }, valueNode, unitNode),
    hintNode,
  );

  return {
    el: root,
    set(value, hint, tone) {
      valueNode.textContent = value;
      if (hint !== undefined) hintNode.textContent = hint;
      if (tone) root.className = `stat stat--${tone}`;
    },
  };
}

export interface SliderOptions {
  min: number;
  max: number;
  step?: number;
  value: number;
  label?: string;
  suffix?: string;
  onInput?: (value: number) => void;
  onCommit?: (value: number) => void;
}

export interface Slider {
  el: HTMLElement;
  setValue(value: number): void;
  setDisabled(disabled: boolean): void;
  value(): number;
}

export function slider(options: SliderOptions): Slider {
  const output = el("output", { class: "slider-value" }, `${Math.round(options.value)}${options.suffix ?? ""}`);
  const input = el("input", {
    class: "slider-input",
    type: "range",
    min: options.min,
    max: options.max,
    step: options.step ?? 1,
    value: options.value,
  });

  const sync = (value: number, commit: boolean) => {
    const clamped = clamp(Math.round(value), options.min, options.max);
    output.textContent = `${clamped}${options.suffix ?? ""}`;
    input.value = String(clamped);
    (commit ? options.onCommit : options.onInput)?.(clamped);
  };

  input.addEventListener("input", () => sync(Number(input.value), false));
  input.addEventListener("change", () => sync(Number(input.value), true));

  const root = el(
    "label",
    { class: "slider" },
    options.label ? el("span", { class: "slider-label" }, options.label, output) : null,
    input,
  );

  return {
    el: root,
    setValue(value) {
      const clamped = clamp(Math.round(value), options.min, options.max);
      input.value = String(clamped);
      output.textContent = `${clamped}${options.suffix ?? ""}`;
    },
    setDisabled(disabled) {
      input.disabled = disabled;
    },
    value() {
      return Number(input.value);
    },
  };
}

export interface ToggleOptions {
  label: string;
  description?: string;
  checked: boolean;
  onChange?: (checked: boolean) => void;
}

export interface Toggle {
  el: HTMLElement;
  setChecked(checked: boolean): void;
}

export function toggle(options: ToggleOptions): Toggle {
  const input = el("input", { type: "checkbox", class: "switch-input", checked: options.checked });
  input.addEventListener("change", () => options.onChange?.(input.checked));

  const root = el(
    "label",
    { class: "field-row" },
    el(
      "span",
      { class: "field-copy" },
      el("span", { class: "field-label" }, options.label),
      options.description ? el("span", { class: "field-help" }, options.description) : null,
    ),
    el("span", { class: "switch" }, input, el("span", { class: "switch-track" })),
  );

  return {
    el: root,
    setChecked(checked) {
      input.checked = checked;
    },
  };
}

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  hint?: string;
}

export function select<T extends string>(options: {
  value: T;
  options: SelectOption<T>[];
  onChange?: (value: T) => void;
  label?: string;
}): { el: HTMLElement; setValue(value: T): void } {
  const node = el(
    "select",
    { class: "select" },
    ...options.options.map((o) => el("option", { value: o.value, selected: o.value === options.value }, o.label)),
  );
  node.addEventListener("change", () => options.onChange?.(node.value as T));
  return {
    el: options.label ? el("label", { class: "field" }, el("span", { class: "field-label" }, options.label), node) : node,
    setValue(value) {
      node.value = value;
    },
  };
}

export function colorField(options: {
  value: string;
  onChange?: (hex: string) => void;
}): { el: HTMLElement; input: HTMLInputElement; setValue(hex: string): void } {
  const input = el("input", { class: "color-input", type: "color", value: options.value });
  input.addEventListener("input", () => options.onChange?.(input.value));
  return {
    el: el("label", { class: "color-field" }, input, el("span", { class: "color-hex" }, options.value.toUpperCase())),
    input,
    setValue(hex) {
      input.value = hex;
      const label = input.parentElement?.querySelector(".color-hex");
      if (label) label.textContent = hex.toUpperCase();
    },
  };
}

export function kv(label: string, value: string | number, mono = true): HTMLElement {
  return el(
    "div",
    { class: "kv" },
    el("span", { class: "kv-key" }, label),
    el("span", { class: `kv-value${mono ? " mono" : ""}` }, String(value)),
  );
}

/** Header strip used by the fan / channel cards. */
export function cardHead(options: {
  title: string;
  subtitle?: string;
  right?: (Node | string)[];
}): HTMLElement {
  return el(
    "div",
    { class: "card-head" },
    el(
      "div",
      { class: "card-titles" },
      el("span", { class: "card-title" }, options.title),
      options.subtitle ? el("span", { class: "card-sub" }, options.subtitle) : null,
    ),
    el("div", { class: "card-right" }, ...(options.right ?? [])),
  );
}

export function inlineNote(text: string, tone: Tone = "muted"): HTMLElement {
  return el("p", { class: `note note--${tone}` }, text);
}

export function emptyState(title: string, detail?: string): HTMLElement {
  return el(
    "div",
    { class: "empty" },
    el("span", { class: "empty-mark" }, "◇"),
    el("p", { class: "empty-title" }, title),
    detail ? el("p", { class: "empty-detail" }, detail) : null,
  );
}

export function bar(percent: number, tone?: string): HTMLElement {
  const fill = el("span", { class: "bar-fill" });
  fill.style.width = `${clamp(percent, 0, 100)}%`;
  if (tone) fill.style.background = tone;
  return el("div", { class: "bar" }, fill);
}

/** Small helper for toggle-groups (segmented buttons). */
export function segmented<T extends string>(options: {
  value: T;
  items: { value: T; label: string }[];
  onChange?: (value: T) => void;
}): { el: HTMLElement; setValue(value: T): void } {
  const buttons = options.items.map((item) =>
    el(
      "button",
      {
        type: "button",
        class: `segment${item.value === options.value ? " is-active" : ""}`,
      },
      item.label,
    ),
  );

  buttons.forEach((node, index) => {
    node.addEventListener("click", () => {
      for (const candidate of buttons) candidate.classList.toggle("is-active", candidate === node);
      options.onChange?.(options.items[index].value);
    });
  });

  const root = el("div", { class: "segmented" }, ...buttons);
  return {
    el: root,
    setValue(value) {
      options.items.forEach((item, index) => {
        buttons[index].classList.toggle("is-active", item.value === value);
      });
    },
  };
}
