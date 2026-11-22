<script lang="ts">
  import { onMount } from "svelte";
  import { Button, Card, Navigator } from "@perishlab/design";
  import Trend from "../Trend.svelte";
  import { Failure, decode, instant, read, request, restore, share, type Bucket, type Evidence, type Report, type View } from "../query";

  const returning = "locus.window";
  let view = $state(restore(new URL(location.href)));
  let opened = $state<View | null>(null);
  let report = $state<Report | null>(null);
  let bucket = $state<Bucket | null>(null);
  let raw = $state("");
  let detail = $state("");
  let status = $state("");
  let unauthorized = $state(false);
  let session = $state(false);
  let busy = $state(false);
  let query: AbortController | undefined;
  let evidence: AbortController | undefined;

  onMount(() => {
    const saved = sessionStorage.getItem(returning);
    sessionStorage.removeItem(returning);
    if (!location.search && saved) {
      const url = new URL(saved, location.origin);
      if (url.origin === location.origin && url.pathname === "/") view = restore(url);
    }
    const moved = () => {
      view = restore(new URL(location.href));
      void search(false);
    };
    window.addEventListener("popstate", moved);
    if (view.producer && view.name && view.unit) void search(false);
    return () => {
      window.removeEventListener("popstate", moved);
      query?.abort();
      evidence?.abort();
    };
  });

  async function search(push = true) {
    query?.abort();
    evidence?.abort();
    bucket = null;
    raw = "";
    status = "";
    unauthorized = false;
    report = null;
    opened = null;
    const asked = { ...view };
    const work = new AbortController();
    query = work;
    busy = true;
    try {
      const params = request(asked);
      const path = share(asked);
      if (path !== location.pathname + location.search) {
        if (push) history.pushState(null, "", path);
        else history.replaceState(null, "", path);
      }
      const result = decode<Report>(await read("/api/v1/metrics?" + params, work.signal));
      if (query !== work) return;
      session = true;
      report = result;
      opened = asked;
      if (result.groups.length === 0) status = "No series match this source and window.";
      else if (result.groups.every(group => group.buckets.every(slot => slot.samples === 0))) {
        status = "No samples in this window.";
      }
    } catch (error) {
      if (work.signal.aborted) return;
      status = error instanceof Error ? error.message : "The query failed.";
      unauthorized = error instanceof Failure && error.status === 401;
      if (unauthorized) session = false;
      if (error instanceof Failure && error.status === 403) session = true;
    } finally {
      if (query === work) busy = false;
    }
  }

  function apply(event: SubmitEvent) {
    event.preventDefault();
    void search();
  }

  function login() {
    sessionStorage.setItem(returning, share(view));
    location.assign("/auth/login");
  }

  async function logout() {
    query?.abort();
    evidence?.abort();
    report = null;
    bucket = null;
    opened = null;
    raw = "";
    try {
      const response = await fetch("/auth/logout", { method: "POST", credentials: "same-origin" });
      if (!response.ok) throw new Failure(response.status);
      session = false;
      unauthorized = true;
      status = "Signed out.";
    } catch {
      status = "Sign-out could not be confirmed. Try again.";
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(new URL(share(opened ?? view), location.origin).href);
      status = "View link copied.";
    } catch {
      status = "Copy the view link from the address bar.";
    }
  }

  function inspect(slot: Bucket) {
    evidence?.abort();
    raw = "";
    detail = "";
    bucket = slot;
  }

  function zoom() {
    if (!bucket || !opened) return;
    view = { ...opened, from: instant(bucket.from), to: instant(bucket.to) };
    void search();
  }

  async function record(item: Evidence) {
    if (!opened) return;
    evidence?.abort();
    const work = new AbortController();
    evidence = work;
    raw = "";
    detail = "Reading raw sample…";
    const params = new URLSearchParams({ producer: opened.producer, from: item.at, to: String(BigInt(item.at) + 1n) });
    try {
      const text = await read("/api/v1/atoms?" + params, work.signal);
      const line = text.split("\n").find(row => row && decode<{id: string}>(row).id === item.id);
      if (work.signal.aborted) return;
      if (line) raw = line;
      detail = line ? "" : "The raw sample is no longer retained.";
    } catch (error) {
      if (!work.signal.aborted) detail = error instanceof Error ? error.message : "Evidence read failed.";
    }
  }

  const names: Record<string, string> = {
    observed: "Observed", missing: "Missing", conflict: "Conflict",
    baseline: "Baseline", discontinuous: "Discontinuous", overflow: "Overflow",
  };
</script>

<header>
  <div class="shell head">
    <Navigator mark="locus" surface="dark" title="Locus" home="/" />
    {#if session}
      <Button label="Sign out" look="quiet" press={logout} />
    {:else}
      <Button label="Sign in" look="quiet" press={login} />
    {/if}
  </div>
</header>

<main class="shell">
  <h1>Metrics</h1>
  <form onsubmit={apply}>
    <div class="selectors">
      <label>Producer <input required bind:value={view.producer} placeholder="workshop.cpu" /></label>
      <label>Metric <input required bind:value={view.name} placeholder="container.cpu" /></label>
      <label>Unit <input required bind:value={view.unit} placeholder="cores" /></label>
      <label>Kind
        <select bind:value={view.kind}><option value="gauge">Gauge</option><option value="counter">Counter</option></select>
      </label>
      <label>Group by <input bind:value={view.group} placeholder="workload" /></label>
      <label>Bucket (seconds) <input required inputmode="numeric" bind:value={view.step} /></label>
      <label>From (UTC) <input required bind:value={view.from} /></label>
      <label>To (UTC) <input required bind:value={view.to} /></label>
    </div>
    <div class="actions">
      <Button submit busy={busy} label={busy ? "Loading…" : "Apply window"} />
      {#if opened}<Button label="Copy view link" look="quiet" press={copy} />{/if}
    </div>
  </form>
  <div class="status" role="status">{status}</div>
  {#if unauthorized}
    <Button label="Continue with Ensign" press={login} />
  {/if}
  {#if report && opened}
    {#each report.groups as group (group.label)}
      <section class="panel" aria-label={group.label ?? opened.name}>
        <h2>{group.label ?? opened.name}</h2>
        <p class="muted">{opened.kind === "gauge" ? "Mean" : "Increase"} · {opened.unit}</p>
        {#if group.buckets.length}
          <Trend buckets={group.buckets} label={group.label ?? opened.name} select={inspect} />
          <div class="scroll visible">
            <table>
              <thead><tr><th scope="col">UTC bucket</th><th scope="col">Value</th><th scope="col">Samples</th><th scope="col">Quality</th></tr></thead>
              <tbody>
                {#each group.buckets as slot (slot.from)}
                  <tr>
                    <td><button type="button" class="link" onclick={() => inspect(slot)}>{instant(slot.from)}</button></td>
                    <td>{slot.value === null ? "—" : slot.value.toLocaleString("en", { maximumSignificantDigits: 12 })}</td>
                    <td>{slot.samples}</td>
                    <td class:warn={slot.quality !== "observed"}>{names[slot.quality] ?? slot.quality}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </section>
    {/each}
  {/if}
  {#if bucket}
    <div class="panel" aria-label="Sample evidence">
      <Card title="Sample evidence">
        <p>{instant(bucket.from)} → {instant(bucket.to)} · {names[bucket.quality] ?? bucket.quality}</p>
        <Button label="Zoom to bucket" look="quiet" press={zoom} />
      </Card>
      {#if bucket.evidence.length}
        <div class="scroll">
          <table>
            <thead><tr><th scope="col">Source time</th><th scope="col">Series</th><th scope="col">Sample ID</th></tr></thead>
            <tbody>
              {#each bucket.evidence as item}
                <tr><td>{instant(item.at)}</td><td>{item.series}</td>
                  <td><button type="button" class="link" onclick={() => record(item)}>{item.id}</button></td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p>No sample evidence.</p>
      {/if}
      {#if detail}<p role="status">{detail}</p>{/if}
      {#if raw}<pre>{raw}</pre>{/if}
    </div>
  {/if}
</main>
