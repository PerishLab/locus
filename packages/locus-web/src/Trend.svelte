<script lang="ts">
  import type { Bucket } from "./query";
  let { buckets, label, select }: { buckets: Bucket[]; label: string; select: (bucket: Bucket) => void } = $props();
  const maximum = $derived(Math.max(0, ...buckets.map(bucket => bucket.value ?? 0)));
  const minimum = $derived(Math.min(0, ...buckets.map(bucket => bucket.value ?? 0)));
  const amplitude = $derived(maximum - minimum || 1);
  let canvas: HTMLCanvasElement;
  let size = $state(0);

  $effect(() => {
    const series = buckets;
    const width = size;
    if (!canvas || width < 1) return;
    const scale = Math.max(1, window.devicePixelRatio);
    const height = 160;
    canvas.width = width * scale;
    canvas.height = height * scale;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.scale(scale, scale);
    context.strokeStyle = "#34434a";
    for (const y of [12, 56, 100, 144]) {
      context.beginPath();
      context.moveTo(0, y);
      context.lineTo(width, y);
      context.stroke();
    }
    const step = width / series.length;
    context.fillStyle = "#69dfbc";
    for (const [index, bucket] of series.entries()) {
      if (bucket.value === null) continue;
      const y = 12 + (maximum - bucket.value) * 132 / amplitude;
      const zero = 12 + maximum * 132 / amplitude;
      context.fillRect(index * step, Math.min(y, zero), Math.max(1, step - 2), Math.max(2, Math.abs(zero - y)));
    }
  });

  function pick(event: MouseEvent) {
    const index = Math.min(buckets.length - 1, Math.floor(event.offsetX * buckets.length / size));
    if (buckets[index]) select(buckets[index]);
  }
</script>

<div class="plot" bind:clientWidth={size}>
  <button type="button" class="plot-hit" aria-label={"Inspect first bucket for " + label}
    onclick={(event) => event.detail === 0 ? select(buckets[0]) : pick(event)}>
    <canvas bind:this={canvas} aria-hidden="true"></canvas>
  </button>
</div>
