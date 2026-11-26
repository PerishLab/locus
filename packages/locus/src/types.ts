export type Origin = "explicit" | "generated" | "inherited";

export type Edge = "enter" | "return";

export type Kind = "counter" | "gauge";

export type Level = "info" | "warn" | "error";

export type Choice = {
  role: string;
  key: string;
  origin: Origin;
};

export type Source = {
  file: string;
  line: number;
  column: number;
  module: string;
  function?: string;
  edge?: Edge;
};

export type Atom = {
  at: bigint;
  id: string;
  producer: string;
  context: Record<string, string>;
  choices: Choice[];
  source?: Source;
  payload?: Record<string, unknown>;
};

export type Diagnostic = {
  code: string;
  message: string;
  status?: number;
};

export type Hook = (diagnostic: Diagnostic) => void;

export type Metric = {
  name: string;
  unit: string;
  kind: Kind;
  series: string;
  value: number;
  labels?: Record<string, string>;
  epoch?: string | number;
};

export type Detail = Record<string, string | number | boolean>;

export type Fetcher = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

export type Apply = <This, Args extends unknown[], Ret>(
  target: (this: This, ...args: Args) => Ret,
  context: ClassMethodDecoratorContext<This, (this: This, ...args: Args) => Ret>,
) => (this: This, ...args: Args) => Ret;

export interface Fork {
  <Ret>(fn: () => Ret): Ret;
  <Ret>(parent: string, fn: () => Ret): Ret;
}

export interface Wrapper {
  <This, Args extends unknown[], Ret>(
    target: (this: This, ...args: Args) => Ret,
    context: ClassMethodDecoratorContext<This, (this: This, ...args: Args) => Ret>,
  ): (this: This, ...args: Args) => Ret;
  (name: string): Apply;
  <This, Args extends unknown[], Ret>(
    name: string,
    target: (this: This, ...args: Args) => Ret,
  ): (this: This, ...args: Args) => Ret;
  <This, Args extends unknown[], Ret>(
    target: (this: This, ...args: Args) => Ret,
  ): (this: This, ...args: Args) => Ret;
}

export type Options = {
  producer: string;
  endpoint?: string;
  enabled?: boolean;
  environment?: Record<string, string>;
  hook?: Hook;
  token?: string;
  headers?: Record<string, string>;
  fetcher?: Fetcher;
  database?: IDBFactory | null;
  mode?: "auto" | "stack";
};

export type Entry = {
  seq?: number;
  stored?: boolean;
  line: string;
  bytes: number;
  urgent: boolean;
};

export type Segment = {
  id?: number;
  created: number;
  bytes: number;
  count: number;
  lines: string[];
  sealed: boolean;
  marks?: Record<string, number>;
};

export type Payload = {
  raw: Uint8Array;
  lines: number;
  until?: number;
  segment?: number;
  origin: "ring" | "spool";
};

export type Frame = {
  trace?: string;
  span?: string;
  parent?: string;
  values: Record<string, string>;
};
