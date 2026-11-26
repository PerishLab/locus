import type { Segment } from "../types.js";
import { measure } from "../wire/encode.js";

export const maxlines = 64;
export const maxbytes = 64 * 1024;
export const maxage = 30_000;
const maxtotal = 2 * 1024 * 1024;
const maxsegments = 32;
const ttl = 24 * 60 * 60 * 1000;

export type Pruned = {
  keep: Segment[];
  expired: number;
  rotated: number;
};

export function fits(seg: Segment, bytes: number, now: number, session: string): boolean {
  if (seg.sealed) return false;
  if (seg.session !== session) return false;
  if (seg.count + 1 > maxlines || seg.bytes + bytes > maxbytes) return false;
  return now - seg.created < maxage;
}

export function ready(seg: Segment, seal: boolean, now: number): boolean {
  return seg.sealed || seal || now - seg.created >= maxage;
}

export function prune(object: IDBObjectStore, all: Segment[], now: number): Pruned {
  const keep: Segment[] = [];
  let expired = 0;
  for (const seg of all) {
    if (now - seg.created > ttl && seg.id !== undefined) {
      object.delete(seg.id);
      expired += 1;
    } else {
      keep.push(seg);
    }
  }
  let bytes = keep.reduce((sum, seg) => sum + seg.bytes, 0);
  let rotated = 0;
  while ((keep.length > maxsegments || bytes > maxtotal) && keep.length > 0) {
    const oldest = keep.shift();
    if (oldest && oldest.id !== undefined) {
      object.delete(oldest.id);
      bytes -= oldest.bytes;
      rotated += 1;
    }
  }
  return { keep, expired, rotated };
}

export function trim(
  object: IDBObjectStore,
  keep: Segment[],
  extra: { bytes: number; added: number },
): number {
  let bytes = keep.reduce((sum, seg) => sum + seg.bytes, 0) + extra.bytes;
  let total = keep.length + extra.added;
  let rotated = 0;
  while ((total > maxsegments || bytes > maxtotal) && keep.length > 0) {
    const oldest = keep.shift();
    if (oldest && oldest.id !== undefined) {
      object.delete(oldest.id);
      bytes -= oldest.bytes;
      total -= 1;
      rotated += 1;
    }
  }
  return rotated;
}

export function stamp(seg: Segment, session: string, seq?: number): void {
  if (seq === undefined) return;
  seg.marks = { ...(seg.marks ?? {}), [session]: seq };
}

export function drop(object: IDBObjectStore, all: Segment[], session: string, until: number): void {
  for (const seg of all) {
    if (seg.id === undefined || seg.session !== session) continue;
    const mark = seg.marks?.[session];
    if (mark !== undefined && mark <= until) {
      object.delete(seg.id);
      continue;
    }
    if (!seg.seqs || seg.seqs.length !== seg.lines.length) continue;
    const lines: string[] = [];
    const seqs: number[] = [];
    for (let i = 0; i < seg.lines.length; i += 1) {
      if (seg.seqs[i] > until) {
        lines.push(seg.lines[i]);
        seqs.push(seg.seqs[i]);
      }
    }
    if (lines.length === seg.lines.length) continue;
    if (lines.length === 0) {
      object.delete(seg.id);
    } else {
      seg.lines = lines;
      seg.seqs = seqs;
      seg.count = lines.length;
      seg.bytes = measure(lines.join("")).byteLength;
      object.put(seg);
    }
  }
}
