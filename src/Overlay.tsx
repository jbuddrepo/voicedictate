import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Mic, Sparkles, TriangleAlert } from "lucide-react";
import type { AppStatus } from "./types";

interface StatusPayload { status: AppStatus; text: string }

export default function Overlay() {
  const [payload, setPayload] = useState<StatusPayload>({ status: "idle", text: "Ready" });
  useEffect(() => {
    const unlisten = listen<StatusPayload>("dictation-status", (event) => setPayload(event.payload));
    return () => { unlisten.then((dispose) => dispose()); };
  }, []);
  return (
    <div className={`overlay-pill ${payload.status}`}>
      <span className="overlay-icon">{payload.status === "error" ? <TriangleAlert size={20} /> : payload.status === "cleaning" ? <Sparkles size={20} /> : <Mic size={20} />}</span>
      <div><strong>{payload.text}</strong><small>{payload.status === "listening" ? "Press the shortcut again to stop" : "VoiceDictate"}</small></div>
      {payload.status === "listening" && <span className="level"><i /><i /><i /><i /></span>}
    </div>
  );
}
