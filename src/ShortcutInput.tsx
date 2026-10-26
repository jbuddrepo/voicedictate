import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { displayShortcut, ShortcutCaptureSession } from "./shortcutCapture";

// One queue across mounts: a previous page's delayed resume must never overtake
// the next field's pause (including React StrictMode cleanup).
let captureOperations: Promise<void> = Promise.resolve();
function pauseShortcut(paused: boolean) {
  const operation = captureOperations.catch(() => undefined).then(() => invoke<void>("set_shortcut_capture", { paused }));
  captureOperations = operation;
  return operation;
}

export function ShortcutInput({ value, onChange, disabled = false }: {
  value: string; onChange: (shortcut: string) => void; disabled?: boolean;
}) {
  const original = useRef(value);
  const session = useRef(new ShortcutCaptureSession());
  const generation = useRef(0);
  const [capturing, setCapturing] = useState(false);
  const [ready, setReady] = useState(false);
  const [preview, setPreview] = useState("");
  const [hint, setHint] = useState("Click the field and press your shortcut. Ctrl + Windows is supported.");

  useEffect(() => () => {
    generation.current += 1;
    // A page change can unmount the focused field without firing blur.
    pauseShortcut(false).catch(() => undefined);
  }, []);

  return <label>Global shortcut
    <input readOnly disabled={disabled} value={capturing && preview ? displayShortcut(preview) : displayShortcut(value)}
      aria-describedby="shortcut-capture-hint" aria-label="Global shortcut. Click then press keys to choose."
      onFocus={() => {
        const current = ++generation.current;
        original.current = value; session.current = new ShortcutCaptureSession();
        setCapturing(true); setReady(false); setPreview(""); setHint("Preparing shortcut capture…");
        pauseShortcut(true).then(() => {
          if (generation.current === current) { setReady(true); setHint("Press your keys, then release them. Escape cancels; Tab finishes."); }
        }).catch(() => {
          if (generation.current === current) setHint("Could not pause the shortcut listener. Leave the field and try again.");
        });
      }}
      onBlur={() => {
        generation.current += 1; setCapturing(false); setReady(false); setPreview("");
        pauseShortcut(false).catch(() => setHint("Shortcut listener could not resume. Restart VoiceDictate before dictating."));
      }}
      onKeyDown={(event) => {
        if (event.key === "Tab") return;
        event.preventDefault();
        if (event.key === "Escape") { onChange(original.current); event.currentTarget.blur(); return; }
        if (!ready || event.repeat) return;
        setPreview(session.current.press(event));
      }}
      onKeyUp={(event) => {
        if (!ready || event.key === "Tab" || event.key === "Escape") return;
        event.preventDefault();
        const result = session.current.release(event);
        if (!result) return;
        setPreview("");
        if (result.error) { setHint(result.error); return; }
        onChange(result.shortcut); setHint("Shortcut captured. Click Save to apply, or press another combination.");
      }} />
    <small id="shortcut-capture-hint" aria-live="polite">{hint}</small>
  </label>;
}
