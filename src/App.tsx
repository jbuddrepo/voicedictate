import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { emit } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isRegistered, register, unregister } from "@tauri-apps/plugin-global-shortcut";
import {
  Check,
  BookOpen,
  CircleHelp,
  Clock3,
  Copy,
  Database,
  History,
  Mic,
  RotateCcw,
  Search,
  Settings as SettingsIcon,
  Sparkles,
  Trash2,
  Wifi,
  WifiOff
} from "lucide-react";
import type { AppSettings, AppStatus, DictationResult, HistoryEntry } from "./types";
import { DEFAULT_CLEANUP_PROMPT } from "./types";

const DEFAULT_SETTINGS: AppSettings = {
  shortcut: "Ctrl+Alt+Space",
  microphoneName: null,
  language: "auto",
  model: "tiny",
  transcriptionMode: "local",
  transcriptionProvider: "whisper-cpp",
  transcriptionBaseUrl: "",
  transcriptionModel: "",
  transcriptionTimeoutSeconds: 90,
  transcriptionApiKeyConfigured: false,
  cleanupEnabled: false,
  provider: "none",
  providerBaseUrl: "http://127.0.0.1:11434",
  providerModel: "",
  providerApiKeyConfigured: false,
  cleanupPrompt: DEFAULT_CLEANUP_PROMPT,
  launchAtStartup: false
};

type Page = "home" | "history" | "settings" | "about";

function App() {
  const [page, setPage] = useState<Page>("home");
  const [appVersion, setAppVersion] = useState("0.2.0");
  const [status, setStatus] = useState<AppStatus>("idle");
  const [statusText, setStatusText] = useState("Ready to dictate");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [activeShortcut, setActiveShortcut] = useState(DEFAULT_SETTINGS.shortcut);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [models, setModels] = useState<string[]>([]);
  const [baseInstalled, setBaseInstalled] = useState(false);
  const [installingBase, setInstallingBase] = useState(false);
  const [query, setQuery] = useState("");
  const [connectionState, setConnectionState] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const [transcriptionConnectionState, setTranscriptionConnectionState] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const [message, setMessage] = useState("");
  const pressStartedAt = useRef<number | null>(null);
  const toggleRecording = useRef(false);
  const processing = useRef(false);
  const statusRef = useRef<AppStatus>("idle");

  useEffect(() => { statusRef.current = status; }, [status]);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => undefined);
  }, []);

  useEffect(() => {
    emit("dictation-status", { status, text: statusText }).catch(() => undefined);
    WebviewWindow.getByLabel("overlay").then((overlay) => {
      if (!overlay) return;
      const isHotkeyActive = status === "listening" || status === "transcribing" || status === "cleaning";
      if (isHotkeyActive) overlay.show();
      else overlay.hide();
    }).catch(() => undefined);
  }, [status, statusText]);

  const refreshHistory = useCallback(async () => {
    setHistory(await invoke<HistoryEntry[]>("list_history"));
  }, []);

  useEffect(() => {
    Promise.all([
      invoke<AppSettings>("get_settings"),
      invoke<string[]>("list_microphones"),
      invoke<HistoryEntry[]>("list_history"),
      invoke<boolean>("is_base_model_installed")
    ]).then(([saved, devices, entries, installed]) => {
      setSettings(installed || saved.model !== "base" ? saved : { ...saved, model: "tiny" });
      setActiveShortcut(saved.shortcut);
      setMicrophones(devices);
      setHistory(entries);
      setBaseInstalled(installed);
    }).catch((error) => setMessage(String(error)));
  }, []);

  const stopAndProcess = useCallback(async () => {
    if (processing.current) return;
    processing.current = true;
    setStatus("transcribing");
    setStatusText(settings.transcriptionMode === "local"
      ? "Transcribing locally — this PC may take a moment…"
      : settings.transcriptionMode === "remote"
        ? "Transcribing with the configured server…"
        : "Trying remote transcription…");
    try {
      const result = await invoke<DictationResult>("stop_and_process");
      if (result.warning) {
        setStatus("success");
        setStatusText(result.warning);
      } else if (result.processingMode === "clean") {
        setStatus("cleaning");
        setStatusText("Text cleaned and pasted");
      } else {
        setStatus("success");
        setStatusText("Text pasted");
      }
      await refreshHistory();
      window.setTimeout(() => {
        setStatus("idle");
        setStatusText("Ready to dictate");
      }, 1600);
    } catch (error) {
      setStatus("error");
      setStatusText(String(error));
    } finally {
      processing.current = false;
      toggleRecording.current = false;
    }
  }, [refreshHistory, settings.transcriptionMode]);

  const startRecording = useCallback(async () => {
    if (processing.current || statusRef.current === "listening") return;
    try {
      await invoke("start_recording");
      setStatus("listening");
      setStatusText("Listening…");
    } catch (error) {
      setStatus("error");
      setStatusText(String(error));
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    const shortcut = activeShortcut;
    (async () => {
      if (await isRegistered(shortcut)) await unregister(shortcut);
      await register(shortcut, async (event) => {
        if (disposed) return;
        if (event.state === "Pressed") {
          if (toggleRecording.current || statusRef.current === "listening") {
            await stopAndProcess();
            pressStartedAt.current = null;
            return;
          }
          pressStartedAt.current = performance.now();
          await startRecording();
        } else if (pressStartedAt.current !== null) {
          const heldFor = performance.now() - pressStartedAt.current;
          pressStartedAt.current = null;
          if (heldFor >= 350) await stopAndProcess();
          else toggleRecording.current = true;
        }
      });
    })().catch((error) => {
      setStatus("error");
      setStatusText(`Shortcut unavailable: ${String(error)}`);
    });
    return () => {
      disposed = true;
      unregister(shortcut).catch(() => undefined);
    };
  }, [activeShortcut, startRecording, stopAndProcess]);

  const saveSettings = async () => {
    await invoke("save_settings", { settings });
    setActiveShortcut(settings.shortcut);
    setMessage("Settings saved");
    window.setTimeout(() => setMessage(""), 1800);
  };

  const testProvider = async () => {
    setConnectionState("testing");
    try {
      const discovered = await invoke<string[]>("test_provider", { settings });
      setModels(discovered);
      setConnectionState("ok");
      if (!settings.providerModel && discovered.length) {
        setSettings((current) => ({ ...current, providerModel: discovered[0] }));
      }
    } catch {
      setConnectionState("error");
    }
  };

  const testTranscriptionProvider = async () => {
    setTranscriptionConnectionState("testing");
    try {
      await invoke("test_transcription_provider", { settings });
      setTranscriptionConnectionState("ok");
      setMessage("Transcription server connected");
    } catch (error) {
      setTranscriptionConnectionState("error");
      setMessage(`Transcription connection failed: ${String(error)}`);
    }
  };

  const installBaseModel = async () => {
    setInstallingBase(true);
    setMessage("Downloading and verifying the multilingual Base modelâ€¦");
    try {
      await invoke("install_base_model");
      setBaseInstalled(true);
      setMessage("Base model installed and verified");
    } catch (error) {
      setMessage(`Base model installation failed: ${String(error)}`);
    } finally {
      setInstallingBase(false);
    }
  };

  const visibleHistory = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle ? history.filter((entry) => entry.finalText.toLowerCase().includes(needle)) : history;
  }, [history, query]);

  return (
    <div className="app-shell">
      <aside>
        <div className="brand"><span className="brand-mark"><Mic size={19} /></span><div><strong>VoiceDictate</strong><small>Private voice typing · v{appVersion}</small></div></div>
        <nav>
          <button className={page === "home" ? "active" : ""} onClick={() => setPage("home")}><Mic size={18} /> Dictate</button>
          <button className={page === "history" ? "active" : ""} onClick={() => setPage("history")}><History size={18} /> History</button>
          <button className={page === "settings" ? "active" : ""} onClick={() => setPage("settings")}><SettingsIcon size={18} /> Settings</button>
          <button className={page === "about" ? "active" : ""} onClick={() => setPage("about")}><CircleHelp size={18} /> Help &amp; About</button>
        </nav>
        <div className="privacy-note"><Database size={16} /><span><strong>Local by default</strong><br />Audio is deleted after transcription.</span></div>
      </aside>

      <main>
        {page === "home" && (
          <section className="home-page">
            <div className={`orb ${status}`} onClick={() => status === "listening" ? stopAndProcess() : startRecording()}>
              <Mic size={38} />
              {status === "listening" && <span className="pulse" />}
            </div>
            <h1>{statusText}</h1>
            <p className="muted">Tap <kbd>{activeShortcut}</kbd> to toggle, or hold it while speaking.</p>
            <div className="status-grid">
              <article><span>Transcription</span><strong>{settings.transcriptionMode === "local" ? `Local Whisper ${settings.model === "tiny" ? "Tiny" : "Base"}` : settings.transcriptionMode === "remote" ? "Remote server" : "Automatic"}</strong><small>{settings.transcriptionMode === "local" ? "Audio stays on this PC" : settings.transcriptionMode === "remote" ? "Uses your configured server" : "Remote with local fallback"}</small></article>
              <article><span>AI cleanup</span><strong>{settings.cleanupEnabled ? settings.provider : "Off"}</strong><small>{settings.cleanupEnabled ? settings.providerModel || "Choose a model" : "Raw transcript"}</small></article>
              <article><span>History</span><strong>{history.length} entries</strong><small>Text only</small></article>
            </div>
            {history[0] && <div className="latest"><div><Clock3 size={16} /> Latest dictation</div><p>{history[0].finalText}</p><button onClick={() => navigator.clipboard.writeText(history[0].finalText)}><Copy size={15} /> Copy</button></div>}
          </section>
        )}

        {page === "history" && (
          <section>
            <header className="page-header"><div><h1>History</h1><p>Your text stays on this PC.</p></div><button className="danger" onClick={async () => { await invoke("clear_history"); await refreshHistory(); }}><Trash2 size={16} /> Clear all</button></header>
            <label className="search"><Search size={17} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search dictations" /></label>
            <div className="history-list">
              {visibleHistory.map((entry) => <article key={entry.id}><div className="history-meta"><span>{new Date(entry.createdAt).toLocaleString()}</span><span>{Math.round(entry.durationMs / 1000)} sec · {entry.processingMode}</span></div><p>{entry.finalText}</p><div className="row-actions"><button onClick={() => navigator.clipboard.writeText(entry.finalText)}><Copy size={15} /> Copy</button><button onClick={async () => { await invoke("delete_history", { id: entry.id }); await refreshHistory(); }}><Trash2 size={15} /> Delete</button></div></article>)}
              {!visibleHistory.length && <div className="empty">No dictations found.</div>}
            </div>
          </section>
        )}

        {page === "settings" && (
          <section>
            <header className="page-header"><div><h1>Settings</h1><p>Choose where speech is transcribed and optionally clean the resulting text.</p></div><button className="primary" onClick={saveSettings}><Check size={16} /> Save</button></header>
            <div className="settings-grid">
              <div className="card"><h2>Dictation</h2><label>Global shortcut<input value={settings.shortcut} onChange={(event) => setSettings({ ...settings, shortcut: event.target.value })} /></label><label>Microphone<select value={settings.microphoneName ?? ""} onChange={(event) => setSettings({ ...settings, microphoneName: event.target.value || null })}><option value="">System default</option>{microphones.map((name) => <option key={name}>{name}</option>)}</select></label><div className="two-col"><label>Language<select value={settings.language} onChange={(event) => setSettings({ ...settings, language: event.target.value })}><option value="auto">Auto detect</option><option value="en">English</option><option value="es">Spanish</option><option value="fr">French</option><option value="de">German</option></select></label><label>{settings.transcriptionMode === "automatic" ? "Local fallback model" : "Local speech model"}<select value={settings.model} onChange={(event) => setSettings({ ...settings, model: event.target.value as "tiny" | "base" })}><option value="tiny">Tiny (included)</option><option value="base" disabled={!baseInstalled}>Base {baseInstalled ? "(installed)" : "(download first)"}</option></select></label></div>{!baseInstalled && settings.transcriptionMode !== "remote" && <button className="secondary" disabled={installingBase} onClick={installBaseModel}><Sparkles size={16} /> {installingBase ? "Installing Base model…" : "Install optional Base model"}</button>}</div>
              <div className="card">
                <h2>Transcription location</h2>
                <label>Method<select value={settings.transcriptionMode} onChange={(event) => setSettings({ ...settings, transcriptionMode: event.target.value as AppSettings["transcriptionMode"] })}><option value="local">Local — free and private</option><option value="remote">My server or online provider</option><option value="automatic">Automatic — remote, then local fallback</option></select></label>
                {settings.transcriptionMode === "local" ? <p className="privacy-warning safe"><strong>Audio stays on this PC.</strong> No transcription network request is made.</p> : <>
                  <label>Server type<select value={settings.transcriptionProvider} onChange={(event) => setSettings({ ...settings, transcriptionProvider: event.target.value as AppSettings["transcriptionProvider"] })}><option value="whisper-cpp">whisper.cpp server</option><option value="openai-compatible">OpenAI-compatible audio API</option></select></label>
                  <label>Server URL<input placeholder={settings.transcriptionProvider === "whisper-cpp" ? "http://100.x.x.x:8080" : "https://api.example.com/v1"} value={settings.transcriptionBaseUrl} onChange={(event) => setSettings({ ...settings, transcriptionBaseUrl: event.target.value })} /></label>
                  {settings.transcriptionProvider === "openai-compatible" && <label>Transcription model<input placeholder="whisper-large-v3-turbo or whisper-1" value={settings.transcriptionModel} onChange={(event) => setSettings({ ...settings, transcriptionModel: event.target.value })} /></label>}
                  <label>API key<input type="password" placeholder={settings.transcriptionApiKeyConfigured ? "Saved securely" : "Optional for private servers"} onChange={(event) => invoke("set_transcription_api_key", { apiKey: event.target.value })} /></label>
                  <label>Timeout (seconds)<input type="number" min={5} max={600} value={settings.transcriptionTimeoutSeconds} onChange={(event) => setSettings({ ...settings, transcriptionTimeoutSeconds: Number(event.target.value) || 90 })} /></label>
                  <button className="secondary" onClick={testTranscriptionProvider}>{transcriptionConnectionState === "testing" ? <RotateCcw className="spin" size={16} /> : transcriptionConnectionState === "ok" ? <Wifi size={16} /> : transcriptionConnectionState === "error" ? <WifiOff size={16} /> : <Sparkles size={16} />} {transcriptionConnectionState === "ok" ? "Connected" : transcriptionConnectionState === "error" ? "Connection failed" : "Test transcription server"}</button>
                  <button className="secondary" onClick={() => invoke("open_help_document", { document: "remote-transcription" }).catch((error) => setMessage(String(error)))}><BookOpen size={16} /> Open remote Whisper setup guide</button>
                  <p className="privacy-warning"><strong>Microphone audio will be sent to this server.</strong> VoiceDictate deletes its temporary copy after transcription. Use HTTPS for public providers.</p>
                  {settings.transcriptionMode === "automatic" && <p className="privacy-warning safe">If the server cannot be reached, bundled local Whisper will be used automatically.</p>}
                </>}
              </div>
              <div className="card"><div className="toggle-row"><div><h2>AI cleanup</h2><p>Only transcript text is sent.</p></div><button className={`switch ${settings.cleanupEnabled ? "on" : ""}`} onClick={() => setSettings({ ...settings, cleanupEnabled: !settings.cleanupEnabled })}><span /></button></div><label>Provider<select value={settings.provider} onChange={(event) => setSettings({ ...settings, provider: event.target.value as AppSettings["provider"] })}><option value="none">None</option><option value="ollama">Ollama</option><option value="openai-compatible">OpenAI compatible</option></select></label><label>Base URL<input value={settings.providerBaseUrl} onChange={(event) => setSettings({ ...settings, providerBaseUrl: event.target.value })} /></label><label>Model<input list="provider-models" value={settings.providerModel} onChange={(event) => setSettings({ ...settings, providerModel: event.target.value })} /><datalist id="provider-models">{models.map((model) => <option key={model} value={model} />)}</datalist></label>{settings.provider === "openai-compatible" && <label>API key<input type="password" placeholder={settings.providerApiKeyConfigured ? "Saved securely" : "Optional for private servers"} onChange={(event) => invoke("set_provider_api_key", { apiKey: event.target.value })} /></label>}<button className="secondary" onClick={testProvider}>{connectionState === "testing" ? <RotateCcw className="spin" size={16} /> : connectionState === "ok" ? <Wifi size={16} /> : connectionState === "error" ? <WifiOff size={16} /> : <Sparkles size={16} />} {connectionState === "ok" ? "Connected" : connectionState === "error" ? "Connection failed" : "Test connection"}</button><label>Cleanup instructions<textarea rows={5} value={settings.cleanupPrompt} onChange={(event) => setSettings({ ...settings, cleanupPrompt: event.target.value })} /></label></div>
            </div>
            {message && <div className="toast">{message}</div>}
          </section>
        )}

        {page === "about" && (
          <section>
            <header className="page-header"><div><h1>Help &amp; About</h1><p>VoiceDictate version {appVersion}</p></div></header>
            <div className="about-hero"><span className="brand-mark"><Mic size={24} /></span><div><h2>VoiceDictate {appVersion}</h2><p>Free, local-first Windows dictation with optional private or hosted transcription.</p></div></div>
            <h2 className="section-title">Choose how transcription runs</h2>
            <div className="about-grid">
              <article><strong>Local Whisper</strong><p>Included and ready immediately. Audio stays on this Windows PC. No server, account, API key, internet connection, or usage fee is required.</p></article>
              <article><strong>Your own server</strong><p>Run whisper.cpp on another Windows PC, K11, Linux VM, home server, or NAS. Connect over a trusted LAN or private Tailscale network.</p></article>
              <article><strong>Online provider</strong><p>Use a provider supporting the OpenAI-compatible audio transcription API. The user supplies the HTTPS URL, model, API key, and pays any provider charges.</p></article>
            </div>
            <div className="card help-card">
              <h2>Instructions and review material</h2>
              <p>These documents are bundled with this installation and can be opened without visiting GitHub.</p>
              <div className="guide-actions">
                <button className="secondary" onClick={() => invoke("open_help_document", { document: "remote-transcription" }).catch((error) => setMessage(String(error)))}><BookOpen size={16} /> Remote Whisper setup</button>
                <button className="secondary" onClick={() => invoke("open_help_document", { document: "video-guide" }).catch((error) => setMessage(String(error)))}><BookOpen size={16} /> Installation video checklist</button>
                <button className="secondary" onClick={() => invoke("open_help_document", { document: "building" }).catch((error) => setMessage(String(error)))}><BookOpen size={16} /> Build instructions</button>
              </div>
              <p className="privacy-warning"><strong>Remote and online modes send microphone audio.</strong> Local mode does not. Never expose an unprotected Whisper or Ollama port directly to the public internet.</p>
            </div>
            {message && <div className="toast">{message}</div>}
          </section>
        )}
      </main>
    </div>
  );
}

export default App;
