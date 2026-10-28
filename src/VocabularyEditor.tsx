import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import type { VocabularyEntry } from "./types";
import "./vocabulary.css";

const MAX_ENTRIES = 100;
const MAX_ALIASES = 10;
const MAX_LENGTH = 80;
const CONTROL_CHARACTERS = /[\u0000-\u001f\u007f]/;

function validateEntry(entry: VocabularyEntry, indexLabel: string): string | null {
  const term = entry.term.trim();
  if (!term) return `${indexLabel}: enter a preferred term.`;
  if (CONTROL_CHARACTERS.test(term)) return `${indexLabel}: the preferred term cannot contain control characters.`;
  if (term.length > MAX_LENGTH) return `${indexLabel}: the preferred term must be ${MAX_LENGTH} characters or fewer.`;
  if (entry.mishearings.length > MAX_ALIASES) return `${indexLabel}: use no more than ${MAX_ALIASES} approved mishearings.`;
  for (const alias of entry.mishearings) {
    if (!alias.trim()) return `${indexLabel}: remove blank mishearing lines or enter a phrase.`;
    if (CONTROL_CHARACTERS.test(alias)) return `${indexLabel}: mishearings cannot contain control characters.`;
    if (alias.trim().length > MAX_LENGTH) return `${indexLabel}: each mishearing must be ${MAX_LENGTH} characters or fewer.`;
  }
  return null;
}

export function validateVocabulary(entries: VocabularyEntry[]): string | null {
  if (entries.length > MAX_ENTRIES) return `Personal vocabulary can contain no more than ${MAX_ENTRIES} entries.`;
  for (const [index, entry] of entries.entries()) {
    const error = validateEntry(entry, `Vocabulary entry ${index + 1}`);
    if (error) return error;
  }
  return null;
}

interface VocabularyEditorProps {
  entries: VocabularyEntry[];
  onChange: (entries: VocabularyEntry[]) => void;
}

export function VocabularyEditor({ entries, onChange }: VocabularyEditorProps) {
  const [term, setTerm] = useState("");
  const [aliasesText, setAliasesText] = useState("");
  const [addError, setAddError] = useState("");

  const addEntry = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const mishearings = aliasesText === "" ? [] : aliasesText.split(/\r?\n/).map((alias) => alias.trim());
    const entry = { term: term.trim(), mishearings, enabled: true };
    const error = entries.length >= MAX_ENTRIES
      ? `Personal vocabulary can contain no more than ${MAX_ENTRIES} entries.`
      : validateEntry(entry, "New entry");
    if (error) {
      setAddError(error);
      return;
    }
    onChange([...entries, entry]);
    setTerm("");
    setAliasesText("");
    setAddError("");
  };

  const updateEntry = (index: number, changes: Partial<VocabularyEntry>) => {
    onChange(entries.map((entry, currentIndex) => currentIndex === index ? { ...entry, ...changes } : entry));
  };

  return (
    <section className="card vocabulary-card" aria-labelledby="personal-vocabulary-heading">
      <h2 id="personal-vocabulary-heading">Personal vocabulary</h2>
      <p className="vocabulary-intro">Add preferred names and job terms. In local Whisper mode, enabled terms are best effort recognition hints. In remote modes, approved mishearings are corrected locally after transcription. This glossary stays on this PC and is never uploaded.</p>
      {entries.length > 0 && <div className="vocabulary-list">
        {entries.map((entry, index) => {
          const error = validateEntry(entry, `Entry ${index + 1}`);
          const termId = `vocabulary-term-${index}`;
          const aliasesId = `vocabulary-aliases-${index}`;
          return <article className="vocabulary-entry" key={index}>
            <div className="vocabulary-entry-heading">
              <label className="vocabulary-enabled"><input type="checkbox" checked={entry.enabled} onChange={(event) => updateEntry(index, { enabled: event.target.checked })} /> Enabled</label>
              <button type="button" className="vocabulary-remove" aria-label={`Remove ${entry.term || `entry ${index + 1}`}`} onClick={() => onChange(entries.filter((_, currentIndex) => currentIndex !== index))}><Trash2 size={15} /> Remove</button>
            </div>
            <label htmlFor={termId}>Preferred term<input id={termId} value={entry.term} maxLength={MAX_LENGTH + 1} aria-invalid={Boolean(error && !entry.term.trim())} onChange={(event) => updateEntry(index, { term: event.target.value })} /></label>
            <label htmlFor={aliasesId}>Approved mishearings (optional)<textarea id={aliasesId} rows={2} value={entry.mishearings.join("\n")} aria-invalid={Boolean(error && entry.mishearings.some((alias) => !alias.trim()))} onChange={(event) => updateEntry(index, { mishearings: event.target.value === "" ? [] : event.target.value.split(/\r?\n/) })} /></label>
            {error && <p className="vocabulary-error" role="alert">{error}</p>}
          </article>;
        })}
      </div>}
      <form className="vocabulary-add" onSubmit={addEntry}>
        <label htmlFor="new-vocabulary-term">Preferred term<input id="new-vocabulary-term" value={term} maxLength={MAX_LENGTH + 1} onChange={(event) => setTerm(event.target.value)} placeholder="e.g. AcmeCloud" /></label>
        <label htmlFor="new-vocabulary-aliases">Approved mishearings (optional)<textarea id="new-vocabulary-aliases" rows={3} value={aliasesText} onChange={(event) => setAliasesText(event.target.value)} placeholder={"One phrase per line\nOnly add phrases you approve for correction"} /></label>
        {addError && <p className="vocabulary-error" role="alert">{addError}</p>}
        <div className="vocabulary-add-footer"><small>{entries.length} of {MAX_ENTRIES} entries · terms and phrases up to {MAX_LENGTH} characters · up to {MAX_ALIASES} mishearings each</small><button type="submit" className="secondary" disabled={entries.length >= MAX_ENTRIES}><Plus size={16} /> Add term</button></div>
      </form>
    </section>
  );
}
