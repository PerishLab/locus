export function attach(handler: (unloading: boolean) => void): () => void {
  const doc = typeof document !== "undefined" ? document : undefined;
  const win = typeof window !== "undefined" ? window : undefined;
  const visibility = () => {
    if (doc?.visibilityState === "hidden") handler(false);
  };
  const hide = () => {
    handler(true);
  };
  if (doc && typeof doc.addEventListener === "function") {
    doc.addEventListener("visibilitychange", visibility);
  }
  if (win && typeof win.addEventListener === "function") {
    win.addEventListener("pagehide", hide);
  }
  return () => {
    if (doc && typeof doc.removeEventListener === "function") {
      doc.removeEventListener("visibilitychange", visibility);
    }
    if (win && typeof win.removeEventListener === "function") {
      win.removeEventListener("pagehide", hide);
    }
  };
}
