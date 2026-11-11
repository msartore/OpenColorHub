/** Session temperature history as a native Canvas sparkline. */

import { Canvas } from "nsh";
import type { Accessor } from "nsh";

export function Sparkline(props: { samples: Accessor<number[]> }) {
  return (
    <Canvas
      style={{ width: "100%", height: 46 }}
      draw={(cx, size) => {
        cx.clearRect(0, 0, size.width, size.height);
        const samples = props.samples();
        if (samples.length < 2) {
          cx.strokeStyle = "#1a2128";
          cx.lineWidth = 1;
          cx.beginPath();
          cx.moveTo(0, size.height - 6);
          cx.lineTo(size.width, size.height - 6);
          cx.stroke();
          return;
        }

        const low = Math.min(...samples);
        const high = Math.max(...samples);
        const span = Math.max(1, high - low);
        const x = (index: number) => (index / (samples.length - 1)) * size.width;
        const y = (value: number) => size.height - 5 - ((value - low) / span) * (size.height - 10);

        cx.beginPath();
        samples.forEach((value, index) => {
          if (index === 0) cx.moveTo(x(index), y(value));
          else cx.lineTo(x(index), y(value));
        });
        cx.strokeStyle = "#4ade80";
        cx.lineWidth = 1.5;
        cx.lineJoin = "round";
        cx.stroke();

        cx.lineTo(size.width, size.height);
        cx.lineTo(0, size.height);
        cx.closePath();
        cx.fillStyle = "rgba(74,222,128,0.08)";
        cx.fill();

        const last = samples[samples.length - 1];
        cx.beginPath();
        cx.arc(size.width - 1, y(last), 2.5, 0, Math.PI * 2);
        cx.fillStyle = "#4ade80";
        cx.fill();
      }}
    />
  );
}

