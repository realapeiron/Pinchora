import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import console from "node:console";
import process from "node:process";

const manifest = JSON.parse(readFileSync("dist/.vite/manifest.json", "utf8"));
const entry = manifest["index.html"];
if (!entry?.isEntry || !entry.file.endsWith(".js"))
  throw new Error("JavaScript entry chunk missing from Vite manifest");

const bytes = statSync(join("dist", entry.file)).size;
const budget = 250_000;
console.log(`Main chunk: ${bytes} / ${budget} bytes`);
if (bytes > budget) process.exitCode = 1;
