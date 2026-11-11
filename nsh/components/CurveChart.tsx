/**
 * The four-point curve, drawn natively and edited by pressing or dragging.
 *
 * The geometry mirrors the old SVG editor: 0–110 °C across, 0–100 % up. The
 * clamping of a moved point belongs to domain (`setCurvePoint`); this component
 * only turns pointer positions into a point index and raw values.
 */

import { Canvas, Text, View, signal } from "nsh";
import type { Accessor } from "nsh";

import type { CurvePoint } from "../app/domain/model";

const PAD = { left: 26, right: 10, top: 10, bottom: 18 };
const MAX_TEMP = 110;

export function CurveChart(props: {
  testID: string;
  curve: Accessor<CurvePoint[]>;
  temperature: Accessor<number | null>;
  onMove: (index: number, point: Partial<CurvePoint>) => void;
}) {
  const box = signal({ width: 0, height: 0 });
  const active = signal<number | null>(null);

  const inner = () => {
    const { width, height } = box();
    return {
      width: Math.max(1, width - PAD.left - PAD.right),
      height: Math.max(1, height - PAD.top - PAD.bottom),
    };
  };
  const xFor = (temp: number): number =>
    PAD.left + (Math.min(MAX_TEMP, Math.max(0, temp)) / MAX_TEMP) * inner().width;
  const yFor = (percent: number): number =>
    PAD.top + inner().height - (Math.min(100, Math.max(0, percent)) / 100) * inner().height;
  const tempFor = (x: number): number =>
    Math.min(MAX_TEMP, Math.max(0, ((x - PAD.left) / inner().width) * MAX_TEMP));
  const percentFor = (y: number): number =>
    Math.min(100, Math.max(0, ((PAD.top + inner().height - y) / inner().height) * 100));

  const nearest = (x: number): number => {
    let best = 0;
    let distance = Number.POSITIVE_INFINITY;
    props.curve().forEach((point, index) => {
      const next = Math.abs(xFor(point.temp_c) - x);
      if (next < distance) {
        distance = next;
        best = index;
      }
    });
    return best;
  };

  const edit = (x: number, y: number): void => {
    if (box().width <= 0) return;
    const index = active() ?? nearest(x);
    active.set(index);
    props.onMove(index, {
      temp_c: Math.round(tempFor(x)),
      percent: Math.round(percentFor(y)),
    });
  };

  return (
    <View class="gap-1">
      <Canvas
        testID={props.testID}
        style={{ height: 150, width: "100%" }}
        onLayout={(event) =>
          box.set({ width: event.bounds.size.width, height: event.bounds.size.height })
        }
        onPressDown={(event) => edit(event.position.x, event.position.y)}
        onMouseMove={(event) => {
          if (event.pressedButton === "left" && active() !== null) {
            edit(event.position.x, event.position.y);
          }
        }}
        onPressUp={() => active.set(null)}
        onTouchStart={(event) => edit(event.locationX, event.locationY)}
        onTouchMove={(event) => {
          if (active() !== null) edit(event.locationX, event.locationY);
        }}
        onTouchEnd={() => active.set(null)}
        draw={(cx, size) => {
          const width = size.width;
          const height = size.height;
          const plotWidth = Math.max(1, width - PAD.left - PAD.right);
          const plotHeight = Math.max(1, height - PAD.top - PAD.bottom);
          const x = (temp: number) =>
            PAD.left + (Math.min(MAX_TEMP, Math.max(0, temp)) / MAX_TEMP) * plotWidth;
          const y = (percent: number) =>
            PAD.top + plotHeight - (Math.min(100, Math.max(0, percent)) / 100) * plotHeight;

          cx.clearRect(0, 0, width, height);

          // Grid: every 25 % and every 20 °C.
          cx.strokeStyle = "rgba(255,255,255,0.07)";
          cx.lineWidth = 1;
          for (let percent = 0; percent <= 100; percent += 25) {
            cx.beginPath();
            cx.moveTo(PAD.left, y(percent));
            cx.lineTo(PAD.left + plotWidth, y(percent));
            cx.stroke();
          }
          for (let temp = 0; temp <= MAX_TEMP; temp += 20) {
            cx.beginPath();
            cx.moveTo(x(temp), PAD.top);
            cx.lineTo(x(temp), PAD.top + plotHeight);
            cx.stroke();
          }

          const curve = props.curve();
          if (curve.length === 0) return;

          // Area under the curve.
          cx.beginPath();
          cx.moveTo(x(curve[0].temp_c), y(0));
          for (const point of curve) cx.lineTo(x(point.temp_c), y(point.percent));
          cx.lineTo(x(curve[curve.length - 1].temp_c), y(0));
          cx.closePath();
          cx.fillStyle = "rgba(74,222,128,0.08)";
          cx.fill();

          // The curve itself.
          cx.beginPath();
          curve.forEach((point, index) => {
            if (index === 0) cx.moveTo(x(point.temp_c), y(point.percent));
            else cx.lineTo(x(point.temp_c), y(point.percent));
          });
          cx.strokeStyle = "#4ade80";
          cx.lineWidth = 2;
          cx.lineJoin = "round";
          cx.stroke();

          // The points.
          for (const point of curve) {
            cx.beginPath();
            cx.arc(x(point.temp_c), y(point.percent), 4, 0, Math.PI * 2);
            cx.fillStyle = "#4ade80";
            cx.fill();
            cx.strokeStyle = "#07090b";
            cx.lineWidth = 2;
            cx.stroke();
          }

          // Where the board is now.
          const temperature = props.temperature();
          if (temperature !== null) {
            cx.beginPath();
            cx.moveTo(x(temperature), PAD.top);
            cx.lineTo(x(temperature), PAD.top + plotHeight);
            cx.strokeStyle = "#7dd3fc";
            cx.lineWidth = 1;
            cx.stroke();
          }

          cx.fillStyle = "#6f8090";
          cx.font = "10px system-ui";
          cx.textBaseline = "top";
          cx.fillText("100%", 2, PAD.top - 3);
          cx.fillText("0%", 2, PAD.top + plotHeight - 10);
          cx.textBaseline = "bottom";
          cx.fillText("0 °C", PAD.left, height);
          cx.fillText(`${MAX_TEMP} °C`, PAD.left + plotWidth - 24, height);
        }}
      />
      <Text class="text-[10px] text-muted-foreground">
        Drag a point — it stays between its neighbours.
      </Text>
    </View>
  );
}
