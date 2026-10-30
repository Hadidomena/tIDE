import { invoke } from '@tauri-apps/api/core';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { session } from './session.svelte';

export type ExportFormat = 'int16' | 'int24' | 'float' | 'flac';

export interface ExportStatus {
	recording: boolean;
	path: string;
	format: string;
	durationSecs: number;
}

/**
 * `durationSecs` is the panel selection: -1 = record until stopped, otherwise auto-stop N s.
 * `format` is the target: `flac` records a 16-bit WAV and converts it after the file closes.
 */
export const exporter = $state({
	open: false,
	recording: false,
	converting: false,
	path: '',
	format: 'int16' as ExportFormat,
	durationSecs: -1,
	startedAt: 0,
	elapsedMs: 0,
	error: ''
});

let timer: ReturnType<typeof setInterval> | undefined;

function stopTimer() {
	if (timer) clearInterval(timer);
	timer = undefined;
}

function startTimer() {
	stopTimer();
	exporter.startedAt = Date.now();
	exporter.elapsedMs = 0;
	timer = setInterval(() => {
		exporter.elapsedMs = Date.now() - exporter.startedAt;
		// A fixed duration auto-stops server-side, so the UI just finishes its countdown.
		if (exporter.durationSecs > 0 && exporter.elapsedMs >= exporter.durationSecs * 1000) {
			stopTimer();
			exporter.recording = false;
			void finalizeIfNeeded();
		}
	}, 250);
}

/** For `flac`: convert the just-closed WAV (the backend polls until the header is patched). */
async function finalizeIfNeeded() {
	if (exporter.format !== 'flac' || !exporter.path) return;
	exporter.converting = true;
	try {
		const status = await invoke<ExportStatus>('export_finalize', {
			wavPath: exporter.path,
			target: 'flac',
			deleteSource: true
		});
		exporter.path = status.path;
		session.notify(`converted → ${status.path}`);
	} catch (e) {
		exporter.error = String(e);
	} finally {
		exporter.converting = false;
	}
}

export async function startExport() {
	if (exporter.recording || exporter.converting) return;
	exporter.error = '';
	try {
		const status = await invoke<ExportStatus>('export_start', {
			durationSecs: exporter.durationSecs > 0 ? exporter.durationSecs : null,
			// FLAC is lossless from integer PCM, so record a 16-bit WAV and convert it.
			format: exporter.format === 'flac' ? 'int16' : exporter.format
		});
		exporter.path = status.path;
		exporter.recording = true;
		startTimer();
		session.notify(`recording → ${status.path}`);
	} catch (e) {
		exporter.error = String(e);
	}
}

export async function stopExport() {
	if (!exporter.recording) return;
	stopTimer();
	exporter.recording = false;
	try {
		await invoke('export_stop');
		session.notify(`recording stopped → ${exporter.path}`);
		await finalizeIfNeeded();
	} catch (e) {
		exporter.error = String(e);
	}
}

export async function openExport() {
	if (!exporter.path) return;
	try {
		await revealItemInDir(exporter.path);
	} catch (e) {
		exporter.error = String(e);
	}
}

export function formatElapsed(ms: number) {
	const total = Math.floor(ms / 1000);
	const mm = String(Math.floor(total / 60)).padStart(2, '0');
	const ss = String(total % 60).padStart(2, '0');
	return `${mm}:${ss}`;
}
