import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Mic, Sparkles, TriangleAlert } from "lucide-react";
import type { AppStatus } from "./types";

interface StatusPayload { status: AppStatus; text: string; activationBehavior: "toggle" | "hold"; revision: number }

export default function Overlay() {
  const [payload, setPayload] = useState<StatusPayload>({ status: "idle", text: "Ready", activationBehavior: "toggle", revision: -1 });
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const apply = (next: StatusPayload) => {
      if (!disposed) setPayload((current) => next.revision >= current.revision ? next : current);
    };
    // Listen before reading the snapshot; revisions prevent stale reads winning a race.
    listen<StatusPayload>("dictation-status", (event) => apply(event.payload)).then(async (stop) => {
      if (disposed) { stop(); return; }
      unlisten = stop;
      apply(await invoke<StatusPayload>("get_current_status"));
    }).catch((error) => console.error("VoiceDictate status synchronization failed", error));
    return () => { disposed = true; unlisten?.(); };
  }, []);
  return (
    <div className={`overlay-pill ${payload.status}`}>
      <span className="overlay-icon">{payload.status === "error" ? <TriangleAlert size={20} /> : payload.status === "cleaning" ? <Sparkles size={20} /> : <Mic size={20} />}</span>
      <div role="status" aria-live="polite"><strong title={payload.text}>{payload.text}</strong><small>{payload.status === "listening" ? payload.activationBehavior === "hold" ? "Release the shortcut to finish" : "Press the shortcut again to stop" : "VoiceDictate"}</small></div>
    </div>
  );
}
