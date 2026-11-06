// Shared helpers of the iso tests: the real atlas and the fixtures the Rust tests write.

import { readFileSync } from "node:fs";

import { Art } from "./art";
import { Atlas } from "./atlas";
import { Content } from "./content";
import type { Catalog } from "./content";
import type { Img } from "./image";
import { decodePng } from "./png";
import { type Cursor, type RoomJson, type Scene, buildScene } from "./scene";

const fixtureDir = new URL("./fixtures/", import.meta.url);
const spriteDir = new URL("../../../assets/sprites/", import.meta.url);

export function fixtureBytes(name: string): Uint8Array {
  return new Uint8Array(readFileSync(new URL(name, fixtureDir)));
}

export function fixtureJson<T>(name: string): T {
  return JSON.parse(readFileSync(new URL(name, fixtureDir), "utf8")) as T;
}

export function fixturePng(name: string): Promise<Img> {
  return decodePng(fixtureBytes(name));
}

let atlas: Promise<Atlas> | null = null;

/** The atlas the bot embeds, decoded once per test file. */
export function realAtlas(): Promise<Atlas> {
  atlas ??= (async () => {
    const png = new Uint8Array(readFileSync(new URL("atlas.png", spriteDir)));
    const index = JSON.parse(readFileSync(new URL("atlas.json", spriteDir), "utf8"));
    return new Atlas(await decodePng(png), index);
  })();
  return atlas;
}

export interface SceneFile {
  name: string;
  room: RoomJson;
  options: { edit: boolean; cursor: Cursor | null; selected: string | null };
  character_sprites: Record<string, string>;
  png: string;
  size: [number, number];
  hash: string;
}

export function fixtureContent(): Content {
  return new Content(fixtureJson<Catalog>("content.json"));
}

/** A scene file turned into what the renderer takes, with the character sprites the file lists. */
export async function sceneOf(file: SceneFile, content: Content): Promise<Scene> {
  const art = new Art(await realAtlas(), content);
  for (const [code, png] of Object.entries(file.character_sprites))
    art.setCharacter(code, await fixturePng(png));
  return buildScene(content, art, file.room, file.options);
}
