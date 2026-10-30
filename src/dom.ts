/** Minimal DOM helpers — keeps the views declarative without a framework. */

type Child = Node | string | number | null | undefined | false;
type Attrs = Record<string, unknown>;

function applyAttrs(node: Element, attrs: Attrs | undefined): void {
  if (!attrs) return;
  // SVG geometry (x, y, r, width, ...) is exposed as read-only accessors, so
  // those elements must go through setAttribute no matter what.
  const isSvg = node instanceof SVGElement;
  for (const [key, value] of Object.entries(attrs)) {
    if (value === null || value === undefined || value === false) continue;
    if (key === "class" || key === "className") {
      node.setAttribute("class", String(value));
    } else if (key === "dataset") {
      for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
        if (v !== undefined && v !== null) (node as HTMLElement).dataset[k] = String(v);
      }
    } else if (key === "style" && typeof value === "object") {
      const styleNode = (node as HTMLElement).style;
      for (const [prop, val] of Object.entries(value as Record<string, string>)) {
        if (prop.startsWith("--")) styleNode.setProperty(prop, String(val));
        else Object.assign(styleNode, { [prop]: String(val) });
      }
    } else if (key.startsWith("on") && typeof value === "function") {
      const event = key.slice(2).toLowerCase();
      node.addEventListener(event, value as EventListener);
    } else if (key === "html") {
      node.innerHTML = String(value);
    } else if (!isSvg && key in node && key !== "list" && typeof value !== "string") {
      (node as unknown as Record<string, unknown>)[key] = value;
    } else {
      node.setAttribute(key, value === true ? "" : String(value));
    }
  }
}

function appendChildren(node: Node, children: Child[]): void {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    node.appendChild(typeof child === "string" || typeof child === "number"
      ? document.createTextNode(String(child))
      : child);
  }
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs?: Attrs,
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  applyAttrs(node, attrs);
  appendChildren(node, children);
  return node;
}

const SVG_NS = "http://www.w3.org/2000/svg";

export function svg<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs?: Attrs,
  ...children: Child[]
): SVGElementTagNameMap[K] {
  const node = document.createElementNS(SVG_NS, tag);
  applyAttrs(node, attrs);
  appendChildren(node, children);
  return node;
}

export function clear(node: Element): void {
  while (node.firstChild) node.removeChild(node.firstChild);
}

/** Typed `querySelector` with a required result. */
export function qs<T extends Element>(selector: string): T {
  const node = document.querySelector<T>(selector);
  if (!node) throw new Error(`missing element: ${selector}`);
  return node;
}

export function clamp(value: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, value));
}

export function round(value: number, digits = 0): number {
  const f = 10 ** digits;
  return Math.round(value * f) / f;
}

/** `#rrggbb` from three bytes. */
export function toHex(color: readonly [number, number, number]): string {
  return `#${color.map((c) => clamp(Math.round(c), 0, 255).toString(16).padStart(2, "0")).join("")}`;
}

/** Three bytes from `#rgb` / `#rrggbb` (falls back to black). */
export function fromHex(hex: string): [number, number, number] {
  const clean = hex.trim().replace(/^#/, "");
  const full = clean.length === 3 ? clean.split("").map((c) => c + c).join("") : clean;
  if (!/^[0-9a-fA-F]{6}$/.test(full)) return [0, 0, 0];
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

/** Whether the app runs inside the Tauri webview or a plain browser. */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
