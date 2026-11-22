/**
 * Dashboard gauge: a native Canvas arc with a needle, plus the value over it.
 * Presentation only; the reading comes from the snapshot.
 */

import { Canvas, Text, View } from "nsh";
import type { Accessor } from "nsh";

import type { TempUnit } from "../app/domain/model";
import { MAX_TEMP_C } from "../app/domain/fan";
import { formatTemp, tempColor } from "../app/presentation/format";

const MAX_C = MAX_TEMP_C;

export function TemperatureGauge(props: {
  temperature: Accessor<number | null>;
  unit: Accessor<TempUnit>;
}) {
  const value = () => props.temperature();
  const color = () => tempColor(value());

  return (
    <View style={{ width: 240, height: 154 }}>
      <Canvas
        style={{ width: "100%", height: "100%" }}
        draw={(cx, size) => {
          const centerX = size.width / 2;
          const centerY = size.height - 24;
          const radius = Math.min(size.width / 2 - 14, size.height - 46);
          const ratio = Math.min(1, Math.max(0, (value() ?? 0) / MAX_C));

          cx.clearRect(0, 0, size.width, size.height);

          // Track
          cx.beginPath();
          cx.arc(centerX, centerY, radius, Math.PI, Math.PI * 2);
          cx.strokeStyle = "#1a2128";
          cx.lineWidth = 10;
          cx.lineCap = "round";
          cx.stroke();

          // Value arc
          if (value() !== null) {
            cx.beginPath();
            cx.arc(centerX, centerY, radius, Math.PI, Math.PI + ratio * Math.PI);
            cx.strokeStyle = color();
            cx.lineWidth = 10;
            cx.lineCap = "round";
            cx.stroke();
          }

          // Ticks
          for (let index = 0; index <= 4; index += 1) {
            const angle = Math.PI + (index / 4) * Math.PI;
            const inner = radius - 16;
            const outer = radius - 9;
            cx.beginPath();
            cx.moveTo(centerX + Math.cos(angle) * inner, centerY + Math.sin(angle) * inner);
            cx.lineTo(centerX + Math.cos(angle) * outer, centerY + Math.sin(angle) * outer);
            cx.strokeStyle = index <= Math.round(ratio * 4) ? color() : "#2a343d";
            cx.lineWidth = 2;
            cx.stroke();
          }

          // Needle
          if (value() !== null) {
            const angle = Math.PI + ratio * Math.PI;
            cx.beginPath();
            cx.moveTo(centerX, centerY);
            cx.lineTo(
              centerX + Math.cos(angle) * (radius - 14),
              centerY + Math.sin(angle) * (radius - 14),
            );
            cx.strokeStyle = color();
            cx.lineWidth = 2;
            cx.stroke();

            cx.beginPath();
            cx.arc(centerX, centerY, 3, 0, Math.PI * 2);
            cx.fillStyle = color();
            cx.fill();
          }

          cx.fillStyle = "#6f8090";
          cx.font = "10px system-ui";
          cx.textBaseline = "bottom";
          cx.textAlign = "left";
          cx.fillText(formatTemp(0, props.unit(), 0), centerX - radius - 4, centerY + 12);
          cx.textAlign = "right";
          cx.fillText(formatTemp(MAX_C, props.unit(), 0), centerX + radius + 4, centerY + 12);
        }}
      />
      <View
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          top: 48,
          alignItems: "center",
          gap: 2,
        }}
      >
        <Text class="text-3xl font-semibold" style={{ color: color() }}>
          {formatTemp(value(), props.unit(), 0)}
        </Text>
        <Text class="text-[10px] text-muted-foreground">motherboard</Text>
      </View>
    </View>
  );
}


