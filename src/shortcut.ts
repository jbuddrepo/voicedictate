import type { ActivationBehavior } from "./types";

export const HOLD_RELEASE_GRACE_MS = 250;

// Serialize native events: a release can arrive before microphone startup finishes.
export class DictationShortcut {
  private pressed = false;
  private pressMode: ActivationBehavior = "toggle";
  private disposed = false;
  private releaseGeneration = 0;
  private releaseTimer: ReturnType<typeof setTimeout> | undefined;
  private operations = Promise.resolve();

  constructor(private actions: {
    mode: () => ActivationBehavior;
    listening: () => boolean;
    busy: () => boolean;
    start: () => Promise<void>;
    stop: () => Promise<void>;
    error: (error: unknown) => void;
  }) {}

  handle(state: "Pressed" | "Released") {
    if (this.disposed) return;
    if (state === "Pressed") {
      if (this.actions.busy()) return;
      if (this.pressed) return; // Ignore keyboard repeat.
      this.pressed = true;
      this.releaseGeneration += 1;
      this.pressMode = this.actions.mode();
      clearTimeout(this.releaseTimer);
      const mode = this.pressMode;
      this.enqueue(async () => {
        if (mode === "toggle" && this.actions.listening()) await this.actions.stop();
        else if (!this.actions.listening()) await this.actions.start();
      });
    } else if (this.pressed) {
      this.pressed = false;
      if (this.pressMode === "hold") {
        const generation = this.releaseGeneration;
        this.releaseTimer = setTimeout(() => {
          this.enqueue(async () => {
            if (generation === this.releaseGeneration && !this.pressed && this.actions.listening()) await this.actions.stop();
          });
        }, HOLD_RELEASE_GRACE_MS);
      }
    }
  }

  private enqueue(operation: () => Promise<void>) {
    this.operations = this.operations.then(async () => {
      if (!this.disposed) await operation();
    }).catch(this.actions.error);
  }

  dispose() {
    this.disposed = true;
    clearTimeout(this.releaseTimer);
  }
}
