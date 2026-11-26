export const threshold = 1024;

export async function compress(
  raw: Uint8Array,
): Promise<{ body: Uint8Array; gzip: boolean }> {
  if (raw.byteLength < threshold || typeof globalThis.CompressionStream !== "function") {
    return { body: raw, gzip: false };
  }
  try {
    const stream = new globalThis.CompressionStream("gzip");
    const writer = stream.writable.getWriter();
    const copy = new Uint8Array(raw.byteLength);
    copy.set(raw);
    const written = writer
      .write(copy)
      .then(() => writer.close())
      .catch(() => undefined);
    const reader = stream.readable.getReader();
    const chunks: Uint8Array[] = [];
    let total = 0;
    while (true) {
      const step = await reader.read();
      if (step.done) break;
      chunks.push(step.value);
      total += step.value.byteLength;
    }
    await written;
    const body = new Uint8Array(total);
    let offset = 0;
    for (const chunk of chunks) {
      body.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return { body, gzip: true };
  } catch {
    return { body: raw, gzip: false };
  }
}
