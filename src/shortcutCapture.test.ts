import { describe, expect, it } from "vitest";
import { captureShortcut, displayShortcut, ShortcutCaptureSession, validateCapturedShortcut } from "./shortcutCapture";
const key = (code: string, modifiers = {}) => ({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...modifiers });
describe("shortcut capture", () => {
  it("captures Ctrl + Windows only after both modifiers are released", () => {
    const s = new ShortcutCaptureSession();
    s.press(key("ControlLeft", { ctrlKey: true }));
    expect(s.press(key("MetaLeft", { ctrlKey: true, metaKey: true }))).toBe("Ctrl+Super");
    expect(s.release(key("ControlLeft", { metaKey: true }))).toBeNull();
    expect(s.release(key("MetaLeft"))).toEqual({ shortcut: "Ctrl+Super", error: null });
  });
  it("captures main key and modifiers without losing the chord on release", () => {
    const s = new ShortcutCaptureSession();
    s.press(key("Space", { ctrlKey: true, metaKey: true }));
    expect(s.release(key("Space", { ctrlKey: true, metaKey: true }))).toBeNull();
    expect(s.release(key("MetaLeft", { ctrlKey: true }))).toBeNull();
    expect(s.release(key("ControlLeft"))).toEqual({ shortcut: "Ctrl+Super+Space", error: null });
  });
  it("rejects multiple main keys and starts a fresh capture next time", () => {
    const s = new ShortcutCaptureSession();
    s.press(key("KeyA", { ctrlKey: true })); s.press(key("KeyB", { ctrlKey: true }));
    s.release(key("KeyA")); const invalid = s.release(key("KeyB"));
    expect(invalid?.error).toContain("only one");
    s.press(key("F12")); expect(s.release(key("F12"))).toEqual({ shortcut: "F12", error: null });
  });
  it("rejects single modifiers, plain letters and unsupported keys", () => {
    expect(validateCapturedShortcut("Ctrl")).toContain("two modifier");
    expect(validateCapturedShortcut("KeyA")).toContain("Add Ctrl");
    expect(captureShortcut(key("Unidentified"))).toBeNull();
    const s = new ShortcutCaptureSession(); s.press(key("Unidentified")); expect(s.release(key("Unidentified"))?.error).toContain("not supported");
  });
  it("keeps punctuation syntax valid and labels the Windows key clearly", () => {
    expect(captureShortcut(key("Equal", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+Equal");
    expect(displayShortcut("Ctrl+Super+KeyK")).toBe("Ctrl + Windows + K");
  });
});
