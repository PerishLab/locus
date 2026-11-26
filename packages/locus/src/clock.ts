const million = 1_000_000n;
const drift = 50_000_000n;

function fill(size: number): Uint8Array {
  const bytes = new Uint8Array(size);
  if (typeof globalThis.crypto?.getRandomValues === "function") {
    globalThis.crypto.getRandomValues(bytes);
    return bytes;
  }
  for (let i = 0; i < size; i += 1) {
    bytes[i] = Math.floor(Math.random() * 256);
  }
  return bytes;
}

export function hex(size: number): string {
  const bytes = fill(size);
  let out = "";
  for (let i = 0; i < bytes.length; i += 1) {
    out += bytes[i].toString(16).padStart(2, "0");
  }
  return out;
}

export class Clock {
  private readonly instance: string;
  private sequence = 0n;
  private origin: bigint;
  private base: number;
  private last = 0n;

  constructor() {
    this.instance = hex(8);
    this.origin = BigInt(Date.now()) * million;
    this.base = typeof performance !== "undefined" && typeof performance.now === "function"
      ? performance.now()
      : 0;
  }

  now(): bigint {
    const wall = BigInt(Date.now()) * million;
    let current = wall;
    if (typeof performance !== "undefined" && typeof performance.now === "function") {
      const delta = performance.now() - this.base;
      const whole = Math.trunc(delta);
      const sub = Math.round((delta - whole) * 1_000_000);
      const estimate = this.origin + BigInt(whole) * million + BigInt(sub);
      const gap = estimate > wall ? estimate - wall : wall - estimate;
      if (gap <= drift) {
        current = estimate;
      } else {
        this.origin = wall;
        this.base = performance.now();
        current = wall;
      }
    }
    if (current < this.last) {
      current = this.last;
    }
    this.last = current;
    return current;
  }

  id(): string {
    const seq = this.sequence.toString(16).padStart(16, "0");
    this.sequence += 1n;
    return this.instance + seq;
  }

  trace(): string {
    return hex(16);
  }

  span(): string {
    return hex(8);
  }
}
