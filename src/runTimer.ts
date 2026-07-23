// Pausable run-timer state, mirroring src-tauri/src/run_timer.rs
// (RunTimerState) exactly — snake_case field names are the serde JSON
// wire format. Elapsed time is accumulated_ms from completed running
// stretches plus (now - running_since_ms) for the live stretch, so the
// frontend can tick locally without per-second IPC.

export interface RunTimerState {
  accumulated_ms: number;
  running_since_ms: number | null;
}

/** The never-started state: nothing accumulated, not running. */
export const IDLE_RUN_TIMER: RunTimerState = {
  accumulated_ms: 0,
  running_since_ms: null,
};

export function isRunning(state: RunTimerState): boolean {
  return state.running_since_ms !== null;
}

/**
 * Total elapsed run time at `nowMs`. A `running_since_ms` in the future
 * (clock skew, hand-edited state file) contributes zero rather than a
 * negative stretch.
 */
export function elapsedMs(state: RunTimerState, nowMs: number): number {
  const live =
    state.running_since_ms === null ? 0 : Math.max(0, nowMs - state.running_since_ms);
  return state.accumulated_ms + live;
}

/** Formats milliseconds as "H:MM:SS" — hours unpadded and uncapped. */
export function formatElapsed(elapsed: number): string {
  const totalSeconds = Math.max(0, Math.floor(elapsed / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${hours}:${pad(minutes)}:${pad(seconds)}`;
}
