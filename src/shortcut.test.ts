import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DictationShortcut, HOLD_RELEASE_GRACE_MS } from "./shortcut";
import type { ActivationBehavior } from "./types";

function setup(mode: ActivationBehavior, pendingStart?: Promise<void>) {
  let listening = false;
  const start = vi.fn(async () => { await pendingStart; listening = true; });
  const stop = vi.fn(async () => { listening = false; });
  const error = vi.fn();
  const busy = vi.fn(() => false);
  const shortcut = new DictationShortcut({ mode: () => mode, listening: () => listening, busy, start, stop, error });
  return { shortcut, start, stop, error, busy };
}

describe("dictation shortcut", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("keeps recording after a long tap-mode keypress and stops on the next press", async () => {
    const { shortcut, start, stop } = setup("toggle");
    shortcut.handle("Pressed");
    await vi.advanceTimersByTimeAsync(1000);
    shortcut.handle("Released");
    await vi.advanceTimersByTimeAsync(1000);
    expect(start).toHaveBeenCalledTimes(1);
    expect(stop).not.toHaveBeenCalled();
    shortcut.handle("Pressed");
    await vi.advanceTimersByTimeAsync(0);
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("ignores repeated native press events", async () => {
    const { shortcut, start, stop } = setup("toggle");
    shortcut.handle("Pressed");
    shortcut.handle("Pressed");
    await vi.advanceTimersByTimeAsync(0);
    expect(start).toHaveBeenCalledTimes(1);
    expect(stop).not.toHaveBeenCalled();
  });

  it("includes the end of speech after releasing Hold mode", async () => {
    const { shortcut, stop } = setup("hold");
    shortcut.handle("Pressed");
    await vi.advanceTimersByTimeAsync(0);
    shortcut.handle("Released");
    await vi.advanceTimersByTimeAsync(HOLD_RELEASE_GRACE_MS - 1);
    expect(stop).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("waits for microphone startup when release arrives first", async () => {
    let ready!: () => void;
    const { shortcut, stop } = setup("hold", new Promise<void>((resolve) => { ready = resolve; }));
    shortcut.handle("Pressed");
    shortcut.handle("Released");
    await vi.advanceTimersByTimeAsync(1000);
    expect(stop).not.toHaveBeenCalled();
    ready();
    await vi.advanceTimersByTimeAsync(0);
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("cancels a pending release when the key is pressed again", async () => {
    const { shortcut, start, stop } = setup("hold");
    shortcut.handle("Pressed");
    await vi.advanceTimersByTimeAsync(0);
    shortcut.handle("Released");
    await vi.advanceTimersByTimeAsync(100);
    shortcut.handle("Pressed");
    await vi.advanceTimersByTimeAsync(1000);
    expect(start).toHaveBeenCalledTimes(1);
    expect(stop).not.toHaveBeenCalled();
    shortcut.handle("Released");
    await vi.advanceTimersByTimeAsync(HOLD_RELEASE_GRACE_MS);
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("cancels release work when unregistered", async () => {
    const { shortcut, stop } = setup("hold");
    shortcut.handle("Pressed");
    await vi.advanceTimersByTimeAsync(0);
    shortcut.handle("Released");
    shortcut.dispose();
    await vi.advanceTimersByTimeAsync(1000);
    expect(stop).not.toHaveBeenCalled();
  });

  it("ignores presses while a transcript is being processed", async () => {
    const { shortcut, start, busy } = setup("toggle");
    busy.mockReturnValue(true);
    shortcut.handle("Pressed");
    shortcut.handle("Released");
    await vi.advanceTimersByTimeAsync(0);
    busy.mockReturnValue(false);
    expect(start).not.toHaveBeenCalled();
  });

  it("preserves the latest release grace when an earlier stop is queued during startup", async () => {
    let ready!: () => void;
    const { shortcut, stop } = setup("hold", new Promise<void>((resolve) => { ready = resolve; }));
    shortcut.handle("Pressed");
    shortcut.handle("Released");
    await vi.advanceTimersByTimeAsync(HOLD_RELEASE_GRACE_MS);
    shortcut.handle("Pressed");
    shortcut.handle("Released");
    ready();
    await vi.advanceTimersByTimeAsync(0);
    expect(stop).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(HOLD_RELEASE_GRACE_MS);
    expect(stop).toHaveBeenCalledTimes(1);
  });
});
