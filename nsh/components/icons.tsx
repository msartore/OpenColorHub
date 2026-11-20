/**
 * Tabler outline icons, inlined from `src/components/icons.ts` on `main`.
 * Tabler Icons are MIT licensed, © Paweł Kuna — see NOTICE.md.
 *
 * A 24×24 box, 2 px stroke and round joins, drawn natively from the SVG path
 * data. No icon package (and no web view) is needed at runtime.
 */

import { Canvas, Path2D } from "nsh";

export const TABLER_PATHS = {
  dashboard: [
    "M5 4h4a1 1 0 0 1 1 1v6a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1v-6a1 1 0 0 1 1 -1",
    "M5 16h4a1 1 0 0 1 1 1v2a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1v-2a1 1 0 0 1 1 -1",
    "M15 12h4a1 1 0 0 1 1 1v6a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1v-6a1 1 0 0 1 1 -1",
    "M15 4h4a1 1 0 0 1 1 1v2a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1v-2a1 1 0 0 1 1 -1",
  ],
  fans: [
    "M9 13a3 3 0 1 0 6 0a3 3 0 1 0 -6 0",
    "M14.167 10.5c.722 -1.538 1.156 -3.043 1.303 -4.514c.22 -1.63 -.762 -2.986 -3.47 -2.986s-3.69 1.357 -3.47 2.986c.147 1.471 .581 2.976 1.303 4.514",
    "M13.169 16.751c.97 1.395 2.057 2.523 3.257 3.386c1.3 1 2.967 .833 4.321 -1.512c1.354 -2.345 .67 -3.874 -.85 -4.498c-1.348 -.608 -2.868 -.985 -4.562 -1.128",
    "M8.664 13c-1.693 .143 -3.213 .52 -4.56 1.128c-1.522 .623 -2.206 2.153 -.852 4.498s3.02 2.517 4.321 1.512c1.2 -.863 2.287 -1.991 3.258 -3.386",
  ],
  leds: [
    "M3 12h1m8 -9v1m8 8h1m-15.4 -6.4l.7 .7m12.1 -.7l-.7 .7",
    "M9 16a5 5 0 1 1 6 0a3.5 3.5 0 0 0 -1 3a2 2 0 0 1 -4 0a3.5 3.5 0 0 0 -1 -3",
    "M9.7 17l4.6 0",
  ],
  drivers: [
    "M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2 -2v-2",
    "M7 11l5 5l5 -5",
    "M12 4l0 12",
  ],
  settings: [
    "M10.325 4.317c.426 -1.756 2.924 -1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543 -.94 3.31 .826 2.37 2.37a1.724 1.724 0 0 0 1.065 2.572c1.756 .426 1.756 2.924 0 3.35a1.724 1.724 0 0 0 -1.066 2.573c.94 1.543 -.826 3.31 -2.37 2.37a1.724 1.724 0 0 0 -2.572 1.065c-.426 1.756 -2.924 1.756 -3.35 0a1.724 1.724 0 0 0 -2.573 -1.066c-1.543 .94 -3.31 -.826 -2.37 -2.37a1.724 1.724 0 0 0 -1.065 -2.572c-1.756 -.426 -1.756 -2.924 0 -3.35a1.724 1.724 0 0 0 1.066 -2.573c-.94 -1.543 .826 -3.31 2.37 -2.37c1 .608 2.296 .07 2.572 -1.065",
    "M9 12a3 3 0 1 0 6 0a3 3 0 0 0 -6 0",
  ],
  about: [
    "M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0",
    "M12 9h.01",
    "M11 12h1v4h1",
  ],
  bookmark: ["M9 4h6a2 2 0 0 1 2 2v14l-5 -3.5l-5 3.5v-14a2 2 0 0 1 2 -2"],
  "chevron-down": ["M6 9l6 6 6-6"],
} as const;

export type TablerIconName = keyof typeof TABLER_PATHS;

/** Draws a Tabler outline icon at `size` points, optionally rotated. */
export function TablerIcon(props: {
  name: TablerIconName;
  size?: number;
  color?: string;
  rotate?: number;
}) {
  const size = props.size ?? 18;
  return (
    <Canvas
      style={{ width: size, height: size }}
      draw={(cx, canvasSize) => {
        const paths: readonly string[] = TABLER_PATHS[props.name];
        cx.translate(canvasSize.width / 2, canvasSize.height / 2);
        if (props.rotate) cx.rotate(props.rotate);
        cx.scale(canvasSize.width / 24, canvasSize.height / 24);
        cx.translate(-12, -12);
        cx.strokeStyle = props.color ?? "#4ade80";
        cx.lineWidth = 2;
        cx.lineCap = "round";
        cx.lineJoin = "round";
        for (const path of paths) cx.stroke(new Path2D(path));
      }}
    />
  );
}
