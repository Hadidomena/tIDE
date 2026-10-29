import { invoke } from '@tauri-apps/api/core';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { session } from './session.svelte';

export type ExportFormat = 'int16' | 'int24' | 'float';

export interface ExportStatus {
	recording: boolean;
	path: string;
	format: string;
	durationSecs: number;
}

export const exporter = $state({
	open: false,
	recording: false,
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
		if (exporter.durationSecs > 0 && exporter.elapsedMs >= exporter.durationSecs * 1000) {
			stopTimer();
			exporter.recording = false;
		}
	}, 250);
}

export async function startExport() {
	if (exporter.recording) return;
	exporter.error = '';
	try {
		const status = await invoke<ExportStatus>('export_start', {
			durationSecs: exporter.durationSecs > 0 ? exporter.durationSecs : null,
			format: exporter.format
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
