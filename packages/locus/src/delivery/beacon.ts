import type { Diagnostic, Fetcher, Payload } from "../types.js";
import { compress } from "../wire/compress.js";
import { digest } from "../wire/hash.js";
import { chunk, type Ring } from "./ring.js";

export const keepalive = 48 * 1024;

export type Target = {
  endpoint: string;
  headers: (key: string, gzip: boolean) => Record<string, string>;
  fetcher: Fetcher;
  notify: (diagnostic: Diagnostic) => void;
  ack?: (until: number) => Promise<void> | void;
};

type Encoded = {
  payload: Payload;
  key: string;
  body: Uint8Array;
  gzip: boolean;
};

async function fit(ring: Ring, notify: (diagnostic: Diagnostic) => void): Promise<Encoded | null> {
  let payload = ring.peek(chunk);
  while (payload) {
    const key = await digest(payload.raw);
    const { body, gzip } = await compress(payload.raw);
    if (body.byteLength <= keepalive) {
      return { payload, key, body, gzip };
    }
    if (payload.lines <= 1) {
      notify({
        code: "lifecycle.oversize",
        message: "keepalive batch exceeds 48 KiB limit",
      });
      return null;
    }
    const next = Math.max(1, Math.floor(payload.lines / 2));
    payload = ring.peek(keepalive, next);
  }
  return null;
}

export async function dispatch(ring: Ring, target: Target): Promise<void> {
  try {
    const fitted = await fit(ring, target.notify);
    if (!fitted) return;
    const headers = target.headers(fitted.key, fitted.gzip);
    const response = await target.fetcher(target.endpoint, {
      method: "POST",
      keepalive: true,
      headers,
      body: fitted.body as unknown as BodyInit,
    });
    if (response.status >= 200 && response.status < 300 && fitted.payload.until !== undefined) {
      ring.drop(fitted.payload.until);
      await target.ack?.(fitted.payload.until);
    }
  } catch {
    return;
  }
}
