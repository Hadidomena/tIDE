import type { Playback } from './session.svelte';

export const BEATS_PER_CYCLE = 4;
const STALE_MS = 1500;

export const playback = $state({
	cycle: 0,
	beat: 0,
	beatFraction: 0,
	cps: 0,
	fresh: false
});

let anchorCycle = 0;
let anchorAt = 0;
let lastSyncAt = 0;

export function syncClock(snapshot: Playback): void {
	const now = performance.now();
	anchorCycle = snapshot.cycle;
	anchorAt = now + snapshot.latency * 1000;
	if (snapshot.cps > 0) playback.cps = snapshot.cps;
	lastSyncAt = now;
}

export function projectCycle(now = performance.now()): number {
	return anchorCycle + ((now - anchorAt) / 1000) * playback.cps;
}

export function isFresh(now = performance.now()): boolean {
	return lastSyncAt > 0 && now - lastSyncAt < STALE_MS;
}

export function beatOf(cycle: number): number {
	const beats = Math.floor(cycle * BEATS_PER_CYCLE);
	return ((beats % BEATS_PER_CYCLE) + BEATS_PER_CYCLE) % BEATS_PER_CYCLE;
}

export function beatFractionOf(cycle: number): number {
	const beats = cycle * BEATS_PER_CYCLE;
	return beats - Math.floor(beats);
}

export function resetClock(): void {
	anchorCycle = 0;
	anchorAt = performance.now();
	lastSyncAt = 0;
	playback.cycle = 0;
	playback.beat = 0;
	playback.beatFraction = 0;
	playback.cps = 0;
	playback.fresh = false;
}
