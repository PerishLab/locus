import { hex } from "../clock.js";
import type { Diagnostic, Entry, Fetcher, Payload } from "../types.js";
import { compress } from "../wire/compress.js";
import { digest } from "../wire/hash.js";
import { dispatch } from "./beacon.js";
import { acquire, backoff, floor } from "./retry.js";
import { Ring } from "./ring.js";
import { Spool } from "./spool.js";

const normal = 2000;
const urgent = 250;

type Config = {
  producer: string;
  endpoint: string;
  token?: string;
  headers?: Record<string, string>;
  fetcher: Fetcher;
  database: IDBFactory | null;
  notify: (diagnostic: Diagnostic) => void;
};

type Outcome = "ack" | "quarantine" | "pause" | "retry";

type Result = {
  outcome: Outcome;
  delay?: number;
  status?: number;
};

export class Transport {
  private readonly config: Config;
  private readonly ring: Ring;
  private readonly spool: Spool;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private deadline = 0;
  private hold = 0;
  private inflight: Promise<void> | null = null;
  private beaconing = false;
  private attempt = 0;
  private paused = false;
  private closed = false;

  constructor(config: Config) {
    this.config = config;
    this.ring = new Ring((dropped) => {
      this.config.notify({
        code: "ring.overflow",
        message: `dropped ${dropped} oldest buffered records`,
      });
    });
    this.spool = new Spool({
      producer: config.producer,
      session: hex(8),
      factory: config.database,
      notify: config.notify,
    });
    if (this.spool.available()) {
      this.schedule(normal);
    }
  }

  enqueue(entry: Entry): void {
    if (this.closed) return;
    const seq = this.ring.push(entry);
    if (this.spool.available()) {
      void this.spool.append({ ...entry, seq }).then((saved) => {
        if (saved) this.ring.mark(seq);
      });
    }
    if (this.paused) return;
    const now = Date.now();
    if (this.hold > now) {
      this.schedule(this.hold - now);
      return;
    }
    if (this.ring.ready()) {
      this.clear();
      void this.drain(true);
      return;
    }
    this.schedule(entry.urgent ? urgent : normal);
  }

  private schedule(delay: number): void {
    if (this.closed || this.paused) return;
    const now = Date.now();
    const target = Math.max(now + delay, this.hold);
    if (this.timer !== null && this.deadline <= target) return;
    this.clear();
    this.deadline = target;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.deadline = 0;
      void this.drain(true);
    }, Math.max(0, target - now));
  }

  private clear(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
      this.deadline = 0;
    }
  }

  private headers(key: string, gzip: boolean): Record<string, string> {
    const out: Record<string, string> = {
      ...(this.config.headers ?? {}),
      "Content-Type": "application/x-ndjson",
      "Idempotency-Key": key,
    };
    if (gzip) out["Content-Encoding"] = "gzip";
    if (this.config.token) out["Authorization"] = "Bearer " + this.config.token;
    return out;
  }

  private async transmit(payload: Payload): Promise<Result> {
    const key = await digest(payload.raw);
    const { body, gzip } = await compress(payload.raw);
    const headers = this.headers(key, gzip);
    try {
      const response = await this.config.fetcher(this.config.endpoint, {
        method: "POST",
        headers,
        body: body as unknown as BodyInit,
      });
      return this.classify(response);
    } catch {
      return { outcome: "retry", delay: backoff(this.attempt, null) };
    }
  }

  private classify(response: Response): Result {
    const status = response.status;
    if (status >= 200 && status < 300) return { outcome: "ack", status };
    if (status === 401 || status === 403) return { outcome: "pause", status };
    if (status === 429 || status >= 500) {
      const retry = response.headers?.get?.("retry-after") ?? null;
      return { outcome: "retry", delay: backoff(this.attempt, retry), status };
    }
    return { outcome: "quarantine", status };
  }

  private async pick(seal: boolean): Promise<Payload | null> {
    if (this.spool.available()) {
      await this.spool.settle();
      const seg = await this.spool.next(seal);
      if (seg) return seg;
      return this.ring.next(this.spool.available());
    }
    return this.ring.next(false);
  }

  private async retire(payload: Payload): Promise<void> {
    if (payload.origin === "spool") {
      if (payload.segment !== undefined) await this.spool.remove(payload.segment);
      if (payload.until !== undefined) this.ring.drop(payload.until);
      return;
    }
    this.ring.confirm();
  }

  private async step(seal: boolean): Promise<boolean> {
    const payload = await this.pick(seal);
    if (!payload) return false;
    const result = await this.transmit(payload);
    return this.apply(payload, result);
  }

  private async apply(payload: Payload, result: Result): Promise<boolean> {
    switch (result.outcome) {
      case "ack":
        this.attempt = 0;
        this.hold = 0;
        await this.retire(payload);
        return true;
      case "quarantine":
        this.attempt = 0;
        this.hold = 0;
        await this.retire(payload);
        this.config.notify({
          code: "transport.quarantine",
          message: `quarantined batch with status ${result.status ?? 400}`,
          status: result.status,
        });
        return true;
      case "pause":
        this.paused = true;
        this.hold = 0;
        this.clear();
        this.config.notify({
          code: "transport.paused",
          message: `sender paused with status ${result.status ?? 401}`,
          status: result.status,
        });
        return false;
      case "retry": {
        const delay = result.delay ?? floor;
        this.attempt += 1;
        this.hold = Date.now() + delay;
        this.config.notify({
          code: "transport.retry",
          message: `batch retry scheduled after ${delay}ms`,
          status: result.status,
        });
        this.schedule(delay);
        return false;
      }
    }
  }

  private drain(seal: boolean): Promise<void> {
    if (this.paused || this.closed) return Promise.resolve();
    const now = Date.now();
    if (this.hold > now) {
      this.schedule(this.hold - now);
      return Promise.resolve();
    }
    if (this.inflight) return this.inflight;
    const task = acquire(this.config.producer, async () => {
      while (!this.paused && !this.closed && Date.now() >= this.hold) {
        const progressed = await this.step(seal);
        if (!progressed) break;
      }
    })
      .then((acquired) => {
        if (!acquired && !this.paused && !this.closed) {
          this.schedule(floor);
        }
      })
      .finally(() => {
        this.inflight = null;
      });
    this.inflight = task;
    return task;
  }

  async flush(): Promise<void> {
    if (this.closed || this.paused) return;
    if (this.inflight) {
      await this.inflight;
      if (this.attempt > 0 || this.paused || this.closed) return;
    }
    const now = Date.now();
    if (this.hold > now) {
      this.schedule(this.hold - now);
      return;
    }
    this.clear();
    await this.drain(true);
  }

  beacon(): void {
    if (this.closed || this.paused || this.beaconing) return;
    this.beaconing = true;
    void dispatch(this.ring, {
      endpoint: this.config.endpoint,
      headers: (key, gzip) => this.headers(key, gzip),
      fetcher: this.config.fetcher,
      notify: this.config.notify,
    }).finally(() => {
      this.beaconing = false;
    });
  }

  async close(): Promise<void> {
    if (this.closed) return;
    await this.flush();
    this.closed = true;
    this.clear();
    this.ring.clear();
    await this.spool.close();
  }
}
