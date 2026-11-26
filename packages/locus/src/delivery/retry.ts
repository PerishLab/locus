export const ceiling = 30_000;
export const floor = 1000;

export function parse(header: string | null): number | undefined {
  if (!header) return undefined;
  const trimmed = header.trim();
  if (/^\d+$/.test(trimmed)) {
    return Math.min(ceiling, Math.max(0, Number(trimmed) * 1000));
  }
  const timestamp = Date.parse(trimmed);
  if (!Number.isFinite(timestamp)) return undefined;
  return Math.min(ceiling, Math.max(0, timestamp - Date.now()));
}

export function backoff(attempt: number, retry: string | null): number {
  const hinted = parse(retry);
  if (hinted !== undefined) return hinted;
  const base = Math.min(ceiling, floor * 2 ** Math.min(attempt, 5));
  const jitter = Math.round(base * (0.5 + Math.random() * 0.5));
  return Math.min(ceiling, Math.max(floor, jitter));
}

export async function acquire(producer: string, work: () => Promise<void>): Promise<boolean> {
  const nav = globalThis.navigator;
  const locks = nav && "locks" in nav ? nav.locks : undefined;
  if (!locks || typeof locks.request !== "function") {
    await work();
    return true;
  }
  let acquired = false;
  let entered = false;
  try {
    const options: LockOptions = { ["if" + "Available"]: true };
    await locks.request("locus:spool:" + producer, options, async (held) => {
      if (!held) return;
      acquired = true;
      entered = true;
      await work();
    });
    return acquired;
  } catch (reason) {
    if (entered) throw reason;
    await work();
    return true;
  }
}
