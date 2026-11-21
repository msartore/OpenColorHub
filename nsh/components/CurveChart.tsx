/**
 * The four-point fan curve, drawn natively and edited by dragging a handle.
 *
 * Dragging follows the usual chart guidance: a 44 pt hit target around each
 * point, the grab offset is kept so the point never jumps to the finger, the
 * view claims the responder so the surrounding Scroll cannot steal the drag,
 * and every move is clamped by the domain (`setCurvePoint` in `fan.ts`). A
 * release outside the view ends the drag cleanly.
 */

import { Canvas, Text, View, signal } from "nsh";
import type { Accessor } from "nsh";

import type { CurvePoint } from "../app/domain/model";

const PAD = { left: 26, right: 10, top: 10, bottom: 18 };
const MAX_TEMP = 120;
const HIT_RADIUS = 22; // 44 pt target

/** One finger's drag, captured on press so the target never changes mid-drag. */
interface Grab {
  index: number;
  offsetX: number;
  offsetY: number;
  pointer: number | null;
}

export function CurveChart(props: {
  testID: string;
  curve: Accessor<CurvePoint[]>;
  temperature: Accessor<number | null>;
  onMove: (index: number, point: Partial<CurvePoint>) => void;
}) {
  const box = signal({ width: 0, height: 0 });
  const active = signal<number | null>(null);
  let grab: Grab | null = null;

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

  /** The point under a position, but only within a comfortable hit radius. */
  const hit = (x: number, y: number): number | null => {
    let best: number | null = null;
    let distance = HIT_RADIUS * HIT_RADIUS;
    props.curve().forEach((point, index) => {
      const dx = xFor(point.temp_c) - x;
      const dy = yFor(point.percent) - y;
      const squared = dx * dx + dy * dy;
      if (squared <= distance) {
        distance = squared;
        best = index;
      }
    });
    return best;
  };

  /** Start a drag on a point, remembering where inside it the finger landed. */
  const begin = (x: number, y: number, pointer: number | null): boolean => {
    if (box().width <= 0) return false;
    const index = hit(x, y);
    if (index === null) return false;
    const point = props.curve()[index];
    grab = {
      index,
      offsetX: xFor(point.temp_c) - x,
      offsetY: yFor(point.percent) - y,
      pointer,
    };
    active.set(index);
    return true;
  };

  /** Move the grabbed point, keeping its offset and clamping in the domain. */
  const move = (x: number, y: number, pointer: number | null): void => {
    if (!grab) return;
    if (pointer !== null && grab.pointer !== null && pointer !== grab.pointer) return;
    const point = props.curve()[grab.index];
    if (!point) return;
    const movedX = x + grab.offsetX;
    const movedY = y + grab.offsetY;
    if (Math.abs(movedX - xFor(point.temp_c)) < 0.5 && Math.abs(movedY - yFor(point.percent)) < 0.5) {
      return;
    }
    props.onMove(grab.index, {
      temp_c: Math.round(tempFor(movedX)),
      percent: Math.round(percentFor(movedY)),
    });
  };

  const end = (): void => {
    grab = null;
    active.set(null);
  };

  return (
    <View class="gap-1">
      <View
        style={{ width: "100%", height: 150 }}
        onLayout={(event) =>
          box.set({ width: event.bounds.size.width, height: event.bounds.size.height })
        }
        // Touch: claim the responder only when a point was grabbed, so empty
        // grid space and vertical scrolls still work as before.
        onStartShouldSetResponder={(event) =>
          begin(event.locationX, event.locationY, event.identifier)
        }
        onResponderGrant={() => true}
        onResponderMove={(event) => move(event.locationX, event.locationY, event.identifier)}
        onResponderRelease={end}
        onResponderTerminate={end}
        onTouchCancel={end}
        // Mouse: same controller, with page coordinates converted to local.
        onPressDown={(event) => begin(event.position.x, event.position.y, null)}
        onMouseMove={(event) => {
          if (event.pressedButton === "left") move(event.position.x, event.position.y, null);
        }}
        onMouseUp={end}
        onPressUp={end}
        onMouseLeave={end}
      >
        <Canvas
          testID={props.testID}
          style={{ width: "100%", height: "100%" }}
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

            // The points; the grabbed one gets a larger, brighter ring.
            const current = active();
            curve.forEach((point, index) => {
              const selected = index === current;
              cx.beginPath();
              cx.arc(x(point.temp_c), y(point.percent), selected ? 6 : 4, 0, Math.PI * 2);
              cx.fillStyle = "#4ade80";
              cx.fill();
              cx.strokeStyle = selected ? "#e7eef4" : "#07090b";
              cx.lineWidth = selected ? 2.5 : 2;
              cx.stroke();
            });

            // A small readout while dragging.
            if (current !== null && curve[current]) {
              const point = curve[current];
              const label = `${point.temp_c} °C · ${point.percent}%`;
              cx.fillStyle = "#e7eef4";
              cx.font = "10px system-ui";
              cx.textBaseline = "bottom";
              cx.fillText(label, Math.min(width - 78, x(point.temp_c) + 10), y(point.percent) - 8);
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
            cx.fillText(`${MAX_TEMP} °C`, PAD.left + plotWidth - 26, height);
          }}
        />
      </View>
      <Text class="text-[10px] text-muted-foreground">
        Drag a point — it stays between its neighbours.
      </Text>
    </View>
  );
}
