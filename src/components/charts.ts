/** SVG charts: temperature gauge, SmartFan curve editor, 200-LED preview strip. */

import { clamp, el, svg } from "../dom";
import { tempColor, tempSymbol, tempValue } from "../format";
import type { ChannelStatus, CurvePoint, TempUnit } from "../types";

function polar(cx: number, cy: number, radius: number, angleDeg: number): [number, number] {
  const radians = ((angleDeg - 90) * Math.PI) / 180;
  return [cx + radius * Math.cos(radians), cy + radius * Math.sin(radians)];
}

function arcPath(cx: number, cy: number, radius: number, start: number, end: number): string {
  const [x1, y1] = polar(cx, cy, radius, start);
  const [x2, y2] = polar(cx, cy, radius, end);
  const large = Math.abs(end - start) > 180 ? 1 : 0;
  const sweep = end > start ? 1 : 0;
  return `M ${x1.toFixed(2)} ${y1.toFixed(2)} A ${radius} ${radius} 0 ${large} ${sweep} ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

export interface Gauge {
  el: HTMLElement;
  update(celsius: number | null, unit: TempUnit): void;
}

/**
 * 240° arc gauge. The scale follows the selected unit so the ticks read
 * naturally in °F as well (32..212 instead of 0..100).
 */
export function temperatureGauge(): Gauge {
  const width = 260;
  const height = 172;
  const cx = 130;
  const cy = 128;
  const radius = 92;
  const startAngle = -120;
  const endAngle = 120;

  const root = svg("svg", {
    class: "gauge",
    viewBox: `0 0 ${width} ${height}`,
    role: "img",
    "aria-label": "Motherboard temperature",
  });

  root.appendChild(
    svg("path", {
      class: "gauge-track",
      d: arcPath(cx, cy, radius, startAngle, endAngle),
    }),
  );

  const valueArc = svg("path", { class: "gauge-value", d: "" });
  root.appendChild(valueArc);

  const ticks = svg("g", { class: "gauge-ticks" });
  for (let i = 0; i <= 5; i += 1) {
    const ratio = i / 5;
    const angle = startAngle + (endAngle - startAngle) * ratio;
    const [ox, oy] = polar(cx, cy, radius - 2, angle);
    const [ix, iy] = polar(cx, cy, radius - 10, angle);
    ticks.appendChild(svg("line", { x1: ix, y1: iy, x2: ox, y2: oy }));
  }
  root.appendChild(ticks);

  const labelLayer = svg("g", { class: "gauge-labels" });
  root.appendChild(labelLayer);

  const needle = svg("g", { class: "gauge-needle" });
  const needleLine = svg("line", { x1: cx, y1: cy, x2: cx, y2: cy });
  needle.appendChild(needleLine);
  needle.appendChild(svg("circle", { cx, cy, r: 4.5, class: "gauge-hub" }));
  root.appendChild(needle);

  const valueText = svg("text", {
    x: cx,
    y: cy - 16,
    class: "gauge-value-text",
    "text-anchor": "middle",
  });
  const unitText = svg("text", {
    x: cx,
    y: cy + 4,
    class: "gauge-unit-text",
    "text-anchor": "middle",
  });
  const caption = svg("text", {
    x: cx,
    y: height - 6,
    class: "gauge-caption",
    "text-anchor": "middle",
  });
  root.append(valueText, unitText, caption);

  const wrap = el("div", { class: "gauge-wrap" }, root);

  const update = (celsius: number | null, unit: TempUnit) => {
    const min = unit === "fahrenheit" ? 32 : 0;
    const max = unit === "fahrenheit" ? 212 : 100;
    const value = tempValue(celsius, unit);

    // Tick labels follow the active unit.
    while (labelLayer.firstChild) labelLayer.removeChild(labelLayer.firstChild);
    for (let i = 0; i <= 5; i += 1) {
      const ratio = i / 5;
      const angle = startAngle + (endAngle - startAngle) * ratio;
      const [tx, ty] = polar(cx, cy, radius - 24, angle);
      labelLayer.appendChild(
        svg(
          "text",
          { x: tx, y: ty + 3, "text-anchor": "middle", class: "gauge-tick-label" },
          String(Math.round(min + (max - min) * ratio)),
        ),
      );
    }

    if (value === null) {
      valueArc.setAttribute("d", "");
      valueText.textContent = "—";
      unitText.textContent = tempSymbol(unit);
      caption.textContent = "NO READING";
      needleLine.setAttribute("x2", String(cx));
      needleLine.setAttribute("y2", String(cy - radius + 26));
      needle.style.opacity = "0.25";
      return;
    }

    const ratio = clamp((value - min) / (max - min), 0, 1);
    const angle = startAngle + (endAngle - startAngle) * ratio;
    const [nx, ny] = polar(cx, cy, radius - 20, angle);

    valueArc.setAttribute("d", arcPath(cx, cy, radius, startAngle, angle));
    valueArc.style.stroke = tempColor(celsius);
    needleLine.setAttribute("x2", nx.toFixed(2));
    needleLine.setAttribute("y2", ny.toFixed(2));
    needle.style.opacity = "1";
    valueText.textContent = String(Math.round(value));
    unitText.textContent = tempSymbol(unit);
    caption.textContent = "MOTHERBOARD";
  };

  update(null, "celsius");
  return { el: wrap, update };
}

export interface CurveEditorOptions {
  curve: CurvePoint[];
  temperature?: number | null;
  onChange?: (points: CurvePoint[], committed: boolean) => void;
  width?: number;
  height?: number;
  readonly?: boolean;
}

export interface CurveEditor {
  el: HTMLElement;
  setCurve(points: CurvePoint[]): void;
  setTemperature(temp: number | null): void;
  curve(): CurvePoint[];
}

/**
 * 4-point curve editor. Points can be dragged (temperature along X, duty along
 * Y) or typed into the number fields; temperatures stay strictly ascending so
 * the chip accepts the table.
 */
export function curveEditor(options: CurveEditorOptions): CurveEditor {
  const width = options.width ?? 340;
  const height = options.height ?? 180;
  const pad = { left: 30, right: 12, top: 12, bottom: 24 };
  const plotW = width - pad.left - pad.right;
  const plotH = height - pad.top - pad.bottom;

  let points: CurvePoint[] = options.curve.map((p) => ({ ...p }));
  let temperature = options.temperature ?? null;
  const readonly = options.readonly ?? false;

  const xFor = (temp: number) => pad.left + (clamp(temp, 0, 110) / 110) * plotW;
  const yFor = (percent: number) => pad.top + plotH - (clamp(percent, 0, 100) / 100) * plotH;
  const tempFor = (x: number) => clamp(((x - pad.left) / plotW) * 110, 0, 110);
  const percentFor = (y: number) => clamp(((pad.top + plotH - y) / plotH) * 100, 0, 100);

  const root = svg("svg", {
    class: `curve-chart${readonly ? " curve-chart--readonly" : ""}`,
    viewBox: `0 0 ${width} ${height}`,
    role: "img",
    "aria-label": "Fan curve",
  });

  const grid = svg("g", { class: "curve-grid" });
  for (let i = 0; i <= 5; i += 1) {
    const y = pad.top + (plotH * i) / 5;
    grid.appendChild(svg("line", { x1: pad.left, y1: y, x2: pad.left + plotW, y2: y }));
    const x = pad.left + (plotW * i) / 5;
    grid.appendChild(svg("line", { x1: x, y1: pad.top, x2: x, y2: pad.top + plotH }));
  }
  root.appendChild(grid);

  const axisLabels = svg("g", { class: "curve-axis" });
  axisLabels.appendChild(
    svg("text", { x: 2, y: pad.top + 6, class: "curve-axis-title" }, "DUTY %"),
  );
  axisLabels.appendChild(
    svg("text", {
      x: width - 2,
      y: height - 6,
      "text-anchor": "end",
      class: "curve-axis-title",
    }, "TEMP °C"),
  );
  [0, 20, 40, 60, 80, 100].forEach((tick) => {
    axisLabels.appendChild(
      svg(
        "text",
        { x: pad.left - 6, y: yFor(tick) + 3, "text-anchor": "end", class: "curve-label" },
        String(tick),
      ),
    );
  });
  [0, 25, 50, 75, 100].forEach((tick) => {
    axisLabels.appendChild(
      svg(
        "text",
        { x: xFor((tick / 100) * 110), y: height - 6, "text-anchor": "middle", class: "curve-label" },
        `${tick}°`,
      ),
    );
  });
  root.appendChild(axisLabels);

  const tempMarker = svg("line", {
    class: "curve-temp-marker",
    y1: pad.top,
    y2: pad.top + plotH,
    x1: 0,
    x2: 0,
  });
  root.appendChild(tempMarker);

  const area = svg("path", { class: "curve-area", d: "" });
  const line = svg("path", { class: "curve-line", d: "" });
  root.append(area, line);

  const handles = points.map((_, index) => {
    const g = svg("g", { class: "curve-handle", "data-index": index });
    const hit = svg("circle", { r: 12, class: "curve-hit" });
    const dot = svg("circle", { r: 4.5, class: "curve-dot" });
    g.append(hit, dot);
    root.appendChild(g);
    return g;
  });

  const readout = el("div", { class: "curve-readout" });
  const fields = points.map((point, index) => {
    const tempInput = el("input", {
      class: "mini-input",
      type: "number",
      min: 0,
      max: 110,
      step: 1,
      value: String(point.temp_c),
      "aria-label": `Point ${index + 1} temperature`,
    });
    const pctInput = el("input", {
      class: "mini-input",
      type: "number",
      min: 0,
      max: 100,
      step: 1,
      value: String(point.percent),
      "aria-label": `Point ${index + 1} duty`,
    });
    const commit = () => {
      const next = points.map((p) => ({ ...p }));
      next[index] = {
        temp_c: clamp(Number(tempInput.value) || 0, index === 0 ? 0 : next[index - 1].temp_c + 1, 110),
        percent: clamp(Number(pctInput.value) || 0, 0, 100),
      };
      points = next;
      options.onChange?.(points.map((p) => ({ ...p })), false);
      render();
    };
    tempInput.addEventListener("change", commit);
    pctInput.addEventListener("change", commit);
    const cell = el(
      "div",
      { class: "curve-point" },
      el("span", { class: "curve-point-index" }, `P${index + 1}`),
      el("span", { class: "curve-point-fields" }, tempInput, el("span", { class: "curve-point-sep" }, "°C →"), pctInput, el("span", { class: "curve-point-sep" }, "%")),
    );
    readout.appendChild(cell);
    return { tempInput, pctInput };
  });

  const render = () => {
    const path = points
      .slice()
      .sort((a, b) => a.temp_c - b.temp_c)
      .map((p, i) => `${i === 0 ? "M" : "L"} ${xFor(p.temp_c).toFixed(2)} ${yFor(p.percent).toFixed(2)}`)
      .join(" ");

    const first = points[0];
    const last = points[points.length - 1];
    area.setAttribute(
      "d",
      `${path} L ${xFor(last.temp_c).toFixed(2)} ${pad.top + plotH} L ${xFor(first.temp_c).toFixed(2)} ${pad.top + plotH} Z`,
    );
    line.setAttribute("d", path);

    points.forEach((point, index) => {
      const g = handles[index];
      g.setAttribute("transform", `translate(${xFor(point.temp_c).toFixed(2)}, ${yFor(point.percent).toFixed(2)})`);
      fields[index].tempInput.value = String(point.temp_c);
      fields[index].pctInput.value = String(point.percent);
    });

    if (temperature === null) {
      tempMarker.style.opacity = "0";
    } else {
      tempMarker.style.opacity = "0.7";
      tempMarker.setAttribute("x1", String(xFor(temperature)));
      tempMarker.setAttribute("x2", String(xFor(temperature)));
    }
  };

  let dragIndex: number | null = null;

  if (!readonly) {
    handles.forEach((handle, index) => {
      handle.addEventListener("pointerdown", (event: PointerEvent) => {
        event.preventDefault();
        dragIndex = index;
        try {
          handle.setPointerCapture(event.pointerId);
        } catch {
          /* synthetic or unsupported pointer — dragging still works via hover */
        }
        handle.classList.add("is-dragging");
      });
      handle.addEventListener("pointermove", (event: PointerEvent) => {
        if (dragIndex !== index) return;
        const rect = root.getBoundingClientRect();
        const scaleX = width / rect.width;
        const scaleY = height / rect.height;
        const x = (event.clientX - rect.left) * scaleX;
        const y = (event.clientY - rect.top) * scaleY;

        const lower = index === 0 ? 0 : points[index - 1].temp_c + 1;
        const upper = index === points.length - 1 ? 110 : points[index + 1].temp_c - 1;
        const temp = clamp(Math.round(tempFor(x)), lower, Math.max(lower, upper));
        const percent = clamp(Math.round(percentFor(y)), 0, 100);

        points = points.map((p, i) => (i === index ? { temp_c: temp, percent } : { ...p }));
        render();
        options.onChange?.(points.map((p) => ({ ...p })), false);
      });
      const end = (event: PointerEvent) => {
        if (dragIndex !== index) return;
        dragIndex = null;
        handle.classList.remove("is-dragging");
        try {
          handle.releasePointerCapture(event.pointerId);
        } catch {
          /* capture may never have been taken */
        }
        options.onChange?.(points.map((p) => ({ ...p })), true);
      };
      handle.addEventListener("pointerup", end);
      handle.addEventListener("pointercancel", end);
    });
  }

  render();

  const wrap = el(
    "div",
    { class: "curve-editor" },
    root,
    readonly
      ? el(
          "div",
          { class: "curve-readout curve-readout--readonly" },
          ...points.map((p, i) =>
            el("span", { class: "curve-point-static" }, `P${i + 1} ${p.temp_c}°C → ${p.percent}%`),
          ),
        )
      : readout,
  );

  return {
    el: wrap,
    setCurve(next) {
      points = next.map((p) => ({ ...p }));
      render();
    },
    setTemperature(temp) {
      temperature = temp;
      render();
    },
    curve() {
      return points.map((p) => ({ ...p }));
    },
  };
}

export interface LedStrip {
  el: HTMLElement;
  update(channels: ChannelStatus[]): void;
}

/** 200-LED preview, one rect per LED, grouped per channel. */
export function ledStrip(options: { legend?: boolean } = {}): LedStrip {
  const showLegend = options.legend ?? true;
  const total = 200;
  const width = 620;
  const height = 26;
  const gap = 0.55;
  const cell = width / total;

  const root = svg("svg", {
    class: "led-strip",
    viewBox: `0 0 ${width} ${height}`,
    role: "img",
    "aria-label": "Lighting preview",
    preserveAspectRatio: "none",
  });

  const rects: SVGRectElement[] = [];
  for (let i = 0; i < total; i += 1) {
    const rect = svg("rect", {
      x: (i * cell + gap / 2).toFixed(2),
      y: 4,
      width: (cell - gap).toFixed(2),
      height: height - 8,
      rx: 1,
      class: "led-cell",
      fill: "#1b2229",
    });
    rects.push(rect);
    root.appendChild(rect);
  }

  // Slot markers: every 20 LEDs (one HID packet) gets a hairline.
  for (let i = 20; i < total; i += 20) {
    root.appendChild(
      svg("line", {
        class: "led-slot",
        x1: (i * cell).toFixed(2),
        x2: (i * cell).toFixed(2),
        y1: 0,
        y2: height,
      }),
    );
  }


  const legend = el("div", { class: "led-strip-legend" });
  const wrap = el("div", { class: "led-strip-wrap" }, root, showLegend ? legend : null);
  if (showLegend) {
    legend.append(
      ...Array.from({ length: 6 }, () => el("span", { class: "led-legend" }, "")),
    );
  }

  const ranges = [
    { from: 0, to: 18 },
    { from: 18, to: 19 },
    { from: 19, to: 20 },
    { from: 20, to: 80 },
    { from: 80, to: 140 },
    { from: 140, to: 200 },
  ];

  const update = (channels: ChannelStatus[]) => {
    channels.forEach((channel, index) => {
      const range = ranges[index];
      if (!range) return;
      const lit = channel.enabled && channel.render_hex !== "#000000";
      const color = lit ? channel.render_hex : "#1b2229";
      for (let i = range.from; i < range.to; i += 1) {
        rects[i].setAttribute("fill", color);
        rects[i].classList.toggle("is-off", !lit);
      }
      if (showLegend) {
        const node = legend.children[index] as HTMLElement | undefined;
        if (node) {
          node.textContent = `${channel.label} · ${channel.enabled ? channel.hex.toUpperCase() : "off"}`;
          node.style.setProperty("--swatch", channel.enabled ? channel.hex : "#1b2229");
        }
      }
    });
  };

  return { el: wrap, update };
}

