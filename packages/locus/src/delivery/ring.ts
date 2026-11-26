import type { Entry, Payload } from "../types.js";
import { measure } from "../wire/encode.js";

export const limit = 256;
export const volume = 512 * 1024;
export const count = 64;
export const chunk = 64 * 1024;

type Pinned = {
  entries: Entry[];
  payload: Payload;
};

function build(entries: Entry[]): Payload {
  const until = entries[entries.length - 1]?.seq;
  return {
    raw: measure(entries.map((item) => item.line).join("")),
    lines: entries.length,
    until,
    origin: "ring",
  };
}

export class Ring {
  private readonly items: Entry[] = [];
  private total = 0;
  private cursor = 0;
  private pinned: Pinned | null = null;
  private readonly overflow: (dropped: number) => void;

  constructor(overflow: (dropped: number) => void) {
    this.overflow = overflow;
  }

  push(entry: Entry): number {
    let dropped = 0;
    while (
      this.items.length > 0 &&
      (this.items.length + 1 > limit || this.total + entry.bytes > volume)
    ) {
      const oldest = this.items.shift();
      if (oldest) {
        this.total -= oldest.bytes;
        dropped += 1;
      }
    }
    this.cursor += 1;
    this.items.push({ ...entry, seq: this.cursor, stored: false });
    this.total += entry.bytes;
    if (dropped > 0) this.overflow(dropped);
    return this.cursor;
  }

  mark(seq: number): void {
    for (const item of this.items) {
      if (item.seq === seq) {
        item.stored = true;
        return;
      }
    }
  }

  size(): number {
    return this.items.length;
  }

  bytes(): number {
    return this.total;
  }

  ready(): boolean {
    return this.items.length >= count || this.total >= chunk;
  }

  empty(): boolean {
    return this.pinned === null && this.items.length === 0;
  }

  next(unpersisted: boolean): Payload | null {
    if (this.pinned !== null) return this.pinned.payload;
    if (this.items.length === 0) return null;
    const picked: Entry[] = [];
    let bytes = 0;
    let index = 0;
    while (index < this.items.length && picked.length < count) {
      const candidate = this.items[index];
      if (unpersisted && candidate.stored) {
        index += 1;
        continue;
      }
      if (picked.length > 0 && bytes + candidate.bytes > chunk) break;
      this.items.splice(index, 1);
      this.total -= candidate.bytes;
      picked.push(candidate);
      bytes += candidate.bytes;
    }
    if (picked.length === 0) return null;
    const payload = build(picked);
    this.pinned = { entries: picked, payload };
    return payload;
  }

  peek(maxbytes = chunk, maxlines = count): Payload | null {
    const source = this.pinned !== null ? this.pinned.entries : this.items;
    if (source.length === 0) return null;
    const picked: Entry[] = [];
    let bytes = 0;
    for (const item of source) {
      if (picked.length >= maxlines) break;
      if (picked.length > 0 && bytes + item.bytes > maxbytes) break;
      picked.push(item);
      bytes += item.bytes;
    }
    if (picked.length === 0) return null;
    return build(picked);
  }

  confirm(): void {
    this.pinned = null;
  }

  drop(until: number): void {
    if (this.pinned !== null) {
      const kept = this.pinned.entries.filter((item) => (item.seq ?? 0) > until);
      this.pinned = kept.length > 0 ? { entries: kept, payload: build(kept) } : null;
    }
    let write = 0;
    for (let read = 0; read < this.items.length; read += 1) {
      const item = this.items[read];
      if ((item.seq ?? 0) <= until) {
        this.total -= item.bytes;
      } else {
        this.items[write] = item;
        write += 1;
      }
    }
    this.items.length = write;
  }

  clear(): void {
    this.items.length = 0;
    this.total = 0;
    this.pinned = null;
  }
}
