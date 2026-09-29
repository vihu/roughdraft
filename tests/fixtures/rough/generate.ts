// Generates `reference.json`: rough.js 4.6.4 output for fixed seeds, using
// the option shapes Excalidraw 0.18 passes (`scene/Shape.ts`).
//
// Run from the repo root: `bun tests/fixtures/rough/generate.ts`. Bun
// auto-installs the pinned version and resolves the unbundled sources the way
// Excalidraw's bundler does (extensionless imports).
//
// rough.js is MIT licensed, (c) 2019 Preet Shihn.
import { writeFileSync } from "node:fs";
import { RoughGenerator } from "roughjs@4.6.4/bin/generator.js";

const gen = new RoughGenerator();

type Opts = Record<string, unknown>;

// Mirrors `generateRoughOptions` for a solid-stroke element of the given width.
const excali = (seed: number, strokeWidth: number, extra: Opts = {}): Opts => ({
  seed,
  disableMultiStroke: false,
  strokeWidth,
  fillWeight: strokeWidth / 2,
  hachureGap: strokeWidth * 4,
  roughness: 1,
  stroke: "#1e1e1e",
  preserveVertices: false,
  ...extra,
});

// Dashed/dotted strokes: multi-stroke off, width + 0.5, fill params unchanged.
const dashed = (seed: number, strokeWidth: number, extra: Opts = {}): Opts =>
  excali(seed, strokeWidth, {
    disableMultiStroke: true,
    strokeWidth: strokeWidth + 0.5,
    ...extra,
  });

const roundedRect = (w: number, h: number, r: number) =>
  `M ${r} 0 L ${w - r} 0 Q ${w} 0, ${w} ${r} L ${w} ${h - r} Q ${w} ${h}, ${
    w - r
  } ${h} L ${r} ${h} Q 0 ${h}, 0 ${h - r} L 0 ${r} Q 0 0, ${r} 0`;

const diamond: [number, number][] = [[60, 0], [120, 40], [60, 80], [0, 40]];

const cases: { name: string; call: string; args: unknown[]; options: Opts }[] = [];
const add = (name: string, call: string, args: unknown[], options: Opts) =>
  cases.push({ name, call, args, options });

const SEEDS = [1, 1968410193, 2147483646];
const FILLS: Opts[] = [
  {},
  { fillStyle: "hachure", fill: "#a5d8ff" },
  { fillStyle: "cross-hatch", fill: "#a5d8ff" },
  { fillStyle: "solid", fill: "#a5d8ff" },
];

for (const seed of SEEDS) {
  for (const roughness of [0, 1, 2]) {
    for (const [i, fill] of FILLS.entries()) {
      const o = excali(seed, 2, { roughness, ...fill });
      add(`rect s${seed} r${roughness} f${i}`, "rectangle", [0, 0, 200, 100], o);
      add(`ellipse s${seed} r${roughness} f${i}`, "ellipse", [100, 50, 200, 100], {
        ...o,
        curveFitting: 1,
      });
      add(`diamond s${seed} r${roughness} f${i}`, "polygon", [diamond], o);
    }
  }
  for (const sw of [1, 4]) {
    add(`rect hachure sw${sw} s${seed}`, "rectangle", [0, 0, 150, 90], excali(seed, sw, {
      fillStyle: "hachure",
      fill: "#ffc9c9",
    }));
  }
  add(`rect dashed s${seed}`, "rectangle", [0, 0, 200, 100], dashed(seed, 2));
  add(`rect small s${seed}`, "rectangle", [0, 0, 12, 6], excali(seed, 2, { roughness: 0.5 }));
  add(`rounded rect s${seed}`, "path", [roundedRect(200, 100, 25)], excali(seed, 2, {
    preserveVertices: true,
  }));
  add(`rounded rect hachure s${seed}`, "path", [roundedRect(200, 100, 25)], excali(seed, 2, {
    preserveVertices: true,
    fillStyle: "hachure",
    fill: "#b2f2bb",
  }));
  add(`rounded rect solid s${seed}`, "path", [roundedRect(200, 100, 25)], excali(seed, 2, {
    preserveVertices: true,
    fillStyle: "solid",
    fill: "#b2f2bb",
  }));
  add(`line2 s${seed}`, "linearPath", [[[0, 0], [180, 60]]], excali(seed, 2));
  add(`line4 s${seed}`, "linearPath", [[[0, 0], [80, 40], [120, -30], [200, 10]]], excali(seed, 2));
  add(`line2 dashed s${seed}`, "linearPath", [[[0, 0], [180, 60]]], dashed(seed, 2));
  add(`curve2 s${seed}`, "curve", [[[0, 0], [180, 60]]], excali(seed, 2));
  add(`curve3 s${seed}`, "curve", [[[0, 0], [90, -40], [180, 60]]], excali(seed, 2));
  add(`curve5 s${seed}`, "curve", [[[0, 0], [50, 50], [100, 0], [150, 50], [200, 0]]], excali(seed, 2));
  add(`closed line hachure s${seed}`, "polygon", [[[0, 0], [100, 0], [50, 80], [0, 0]]], excali(seed, 2, {
    fillStyle: "hachure",
    fill: "#ffec99",
  }));
  add(`closed curve solid s${seed}`, "curve", [[[0, 0], [100, 0], [80, 60], [20, 70], [0, 0]]], excali(seed, 2, {
    fillStyle: "solid",
    fill: "#ffec99",
  }));
  add(`closed curve hachure s${seed}`, "curve", [[[0, 0], [100, 0], [80, 60], [20, 70], [0, 0]]], excali(seed, 2, {
    fillStyle: "hachure",
    fill: "#ffec99",
  }));
  add(`arrowhead cap s${seed}`, "line", [170, 50, 200, 60], excali(seed, 2));
  add(`arrowhead dot s${seed}`, "circle", [200, 60, 12], excali(seed, 2, {
    fill: "#1e1e1e",
    fillStyle: "solid",
    roughness: 0.5,
  }));
  add(`arrowhead triangle s${seed}`, "polygon", [[[200, 60], [180, 50], [182, 70], [200, 60]]], excali(seed, 2, {
    fill: "#1e1e1e",
    fillStyle: "solid",
  }));
}

const out = cases.map((c) => {
  const drawable = (gen as any)[c.call](...c.args, c.options);
  return { ...c, sets: drawable.sets.map((s: { type: string; ops: unknown }) => ({ type: s.type, ops: s.ops })) };
});

const path = new URL("./reference.json", import.meta.url);
writeFileSync(path, JSON.stringify({ roughjs: "4.6.4", cases: out }) + "\n");
console.log(`wrote ${out.length} cases`);
