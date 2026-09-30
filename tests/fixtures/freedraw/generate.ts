// Generates `reference.json`: Excalidraw 0.18.1's `getFreeDrawSvgPath` for
// every freedraw element of `tests/fixtures/scenes/freedraw.excalidraw`,
// using perfect-freehand 1.2.0 (the version Excalidraw pins).
//
// Run from the repo root: `bun tests/fixtures/freedraw/generate.ts` (Bun
// auto-installs the pinned version). The function below is copied from
// Excalidraw's `renderer/renderElement.ts` (MIT, Copyright (c) 2020
// Excalidraw); perfect-freehand is MIT, Copyright (c) 2021 Stephen Ruiz Ltd.
import { readFileSync, writeFileSync } from "node:fs";
import { getStroke } from "perfect-freehand@1.2.0";

type Element = {
  id: string;
  type: string;
  points: number[][];
  pressures: number[];
  simulatePressure: boolean;
  strokeWidth: number;
  lastCommittedPoint: number[] | null;
};

function getFreeDrawSvgPath(element: Element) {
  const inputPoints = element.simulatePressure
    ? element.points
    : element.points.length
    ? element.points.map(([x, y], i) => [x, y, element.pressures[i]])
    : [[0, 0, 0.5]];
  const options = {
    simulatePressure: element.simulatePressure,
    size: element.strokeWidth * 4.25,
    thinning: 0.6,
    smoothing: 0.5,
    streamline: 0.5,
    easing: (t: number) => Math.sin((t * Math.PI) / 2),
    last: !!element.lastCommittedPoint,
  };
  return getSvgPathFromStroke(getStroke(inputPoints as number[][], options));
}

function med(A: number[], B: number[]) {
  return [(A[0] + B[0]) / 2, (A[1] + B[1]) / 2];
}

const TO_FIXED_PRECISION = /(\s?[A-Z]?,?-?[0-9]*\.[0-9]{0,2})(([0-9]|e|-)*)/g;

function getSvgPathFromStroke(points: number[][]): string {
  if (!points.length) {
    return "";
  }
  const max = points.length - 1;
  return points
    .reduce(
      (acc: unknown[], point, i, arr) => {
        if (i === max) {
          acc.push(point, med(point, arr[0]), "L", arr[0], "Z");
        } else {
          acc.push(point, med(point, arr[i + 1]));
        }
        return acc;
      },
      ["M", points[0], "Q"],
    )
    .join(" ")
    .replace(TO_FIXED_PRECISION, "$1");
}

const scene = JSON.parse(
  readFileSync("tests/fixtures/scenes/freedraw.excalidraw", "utf8"),
);
const paths: Record<string, string> = {};
for (const element of scene.elements as Element[]) {
  if (element.type === "freedraw") {
    paths[element.id] = getFreeDrawSvgPath(element);
  }
}
writeFileSync(
  "tests/fixtures/freedraw/reference.json",
  JSON.stringify(paths, null, 2) + "\n",
);
