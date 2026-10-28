type KeyEvent = Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">;
const modifiers = ["Ctrl", "Alt", "Shift", "Super"];
const otherKeys = new Set([
  "Backquote", "Backslash", "BracketLeft", "BracketRight", "Comma", "Equal", "Minus",
  "Period", "Quote", "Semicolon", "Slash", "Backspace", "CapsLock", "Enter", "Space",
  "Delete", "End", "Home", "Insert", "PageDown", "PageUp", "PrintScreen", "ScrollLock",
  "Pause", "NumLock", "ArrowDown", "ArrowLeft", "ArrowRight", "ArrowUp", "NumpadAdd",
  "NumpadDecimal", "NumpadDivide", "NumpadMultiply", "NumpadSubtract"
]);

function modifierCode(code: string): boolean { return /^(Control|Shift|Alt|Meta)(Left|Right)$/.test(code); }
function mainKey(code: string): string | null {
  return otherKeys.has(code) || /^(Key[A-Z]|Digit[0-9]|Numpad[0-9]|F([1-9]|1[0-9]|2[0-4]))$/.test(code) ? code : null;
}

export function captureShortcut(event: KeyEvent): string | null {
  const key = mainKey(event.code);
  if (!key && !modifierCode(event.code)) return null;
  return [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift",
    event.metaKey && "Super", key].filter(Boolean).join("+");
}

export function validateCapturedShortcut(value: string): string | null {
  const keys = value.split("+").filter(Boolean);
  if (keys.length === 0) return "Press the keys you want to use.";
  if (keys.length > 4) return "Choose at most four keys.";
  const main = keys.filter((key) => !modifiers.includes(key));
  if (main.length > 1) return "Choose only one letter, number or other main key.";
  if (main.length === 0 && keys.length < 2) return "Use at least two modifier keys, such as Ctrl + Windows.";
  if (main.length === 1 && keys.length === 1 && !/^F([1-9]|1[0-9]|2[0-4])$/.test(main[0])) return "Add Ctrl, Alt, Shift or Windows, or choose a function key.";
  return null;
}

// Capture the whole chord, then finish only once all keys are released. Key
// repeats do not change it, and keyup modifier flags also cover keys held before focus.
export class ShortcutCaptureSession {
  private held = new Set<string>();
  private candidate = "";
  private error: string | null = null;
  press(event: KeyEvent): string {
    this.held.add(event.code);
    const captured = captureShortcut(event);
    if (!captured) { this.error = "This key is not supported. Try another shortcut."; return this.candidate; }
    const current = new Set(this.candidate.split("+").filter(Boolean));
    for (const key of captured.split("+").filter(Boolean)) current.add(key);
    this.candidate = [...modifiers.filter((key) => current.has(key)), ...[...current].filter((key) => !modifiers.includes(key))].join("+");
    return this.candidate;
  }
  release(event: KeyEvent): { shortcut: string; error: string | null } | null {
    this.held.delete(event.code);
    // Lost modifier keyups are reflected in the browser modifier flags.
    if (!event.ctrlKey) { this.held.delete("ControlLeft"); this.held.delete("ControlRight"); }
    if (!event.altKey) { this.held.delete("AltLeft"); this.held.delete("AltRight"); }
    if (!event.shiftKey) { this.held.delete("ShiftLeft"); this.held.delete("ShiftRight"); }
    if (!event.metaKey) { this.held.delete("MetaLeft"); this.held.delete("MetaRight"); }
    if (this.held.size || event.ctrlKey || event.altKey || event.shiftKey || event.metaKey) return null;
    const result = { shortcut: this.candidate, error: this.error ?? validateCapturedShortcut(this.candidate) };
    this.candidate = ""; this.error = null;
    return result;
  }
}

export function displayShortcut(value: string): string {
  return value.split("+").map((key) => /^(Super|Win|Meta|Command|Cmd)$/i.test(key) ? "Windows" : key.replace(/^Key([A-Z])$/, "$1").replace(/^Digit([0-9])$/, "$1")).join(" + ");
}
