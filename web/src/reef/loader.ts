import manifestUrl from "./manifest-url.json";
import { crop, type Img } from "../iso/image";
import { decodePng } from "../iso/png";
import { type AssetManifest, validateManifest } from "./manifest";

/** LRU charged in decoded bytes, not compressed bytes; retained memory never exceeds maxBytes. */
export class ByteCache<T> {
  private entries = new Map<string, { value: T; bytes: number }>();
  bytes = 0;
  readonly maxBytes: number;
  private dispose: (value: T) => void;
  constructor(maxBytes: number, dispose: (value: T) => void = () => {}) {
    this.maxBytes = maxBytes;
    this.dispose = dispose;
  }
  get(key: string): T | undefined {
    const entry = this.entries.get(key);
    if (!entry) return;
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.value;
  }
  set(key: string, value: T, bytes: number): void {
    this.delete(key);
    if (bytes > this.maxBytes) return;
    while (this.bytes + bytes > this.maxBytes) this.delete(this.entries.keys().next().value!);
    this.entries.set(key, { value, bytes });
    this.bytes += bytes;
  }
  delete(key: string): void {
    const entry = this.entries.get(key);
    if (entry) {
      this.bytes -= entry.bytes;
      this.dispose(entry.value);
      this.entries.delete(key);
    }
  }
  clear(): void {
    for (const key of this.entries.keys()) this.delete(key);
  }
}

/** Conservative cold accounting counts actual response bodies, even a cached HTTP response. */
export class ByteLedger {
  bytes = 0;
  firstInteraction: number | null = null;
  async fetch(url: string, signal?: AbortSignal): Promise<Uint8Array<ArrayBuffer>> {
    const response = await fetch(url, { credentials: "same-origin", signal });
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    this.bytes += bytes.byteLength;
    return bytes;
  }
  async json<T>(url: string, signal?: AbortSignal): Promise<T> {
    return JSON.parse(new TextDecoder().decode(await this.fetch(url, signal))) as T;
  }
  markPlayable(): void {
    this.firstInteraction ??= this.bytes;
  }
}

export class AssetLoader {
  readonly cache = new ByteCache<Img>(16 * 1024 * 1024);
  private pending = new Map<string, Promise<Img>>();
  readonly manifest: AssetManifest;
  readonly ledger: ByteLedger;
  readonly signal?: AbortSignal;
  constructor(manifest: AssetManifest, ledger = new ByteLedger(), signal?: AbortSignal) {
    this.manifest = manifest;
    this.ledger = ledger;
    this.signal = signal;
  }
  static async open(ledger: ByteLedger, signal?: AbortSignal): Promise<AssetLoader> {
    return new AssetLoader(
      validateManifest(await ledger.json<AssetManifest>(manifestUrl, signal)),
      ledger,
      signal,
    );
  }
  async page(id: string): Promise<Img> {
    const hit = this.cache.get(id);
    if (hit) return hit;
    const running = this.pending.get(id);
    if (running) return running;
    const p = this.manifest.pages[id];
    if (!p) throw new Error(`Unknown page: ${id}`);
    const task = (async () => {
      const bytes = await this.ledger.fetch(p.url, this.signal);
      if (bytes.length !== p.bytes) throw new Error("Reef page revision mismatch");
      const image = await decodePng(bytes);
      if (image.width !== p.width || image.height !== p.height)
        throw new Error("Reef page dimensions mismatch");
      if (!this.signal?.aborted) this.cache.set(id, image, image.data.byteLength);
      return image;
    })();
    this.pending.set(id, task);
    try {
      return await task;
    } finally {
      this.pending.delete(id);
    }
  }
  async sprite(name: string): Promise<Img | null> {
    const entry = this.manifest.sprites[name];
    if (!entry) return null;
    return crop(await this.page(entry.page), ...entry.rect);
  }
  clear(): void {
    this.cache.clear();
  }
}
