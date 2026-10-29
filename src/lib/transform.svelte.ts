import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getDb } from './db';
import { session } from './session.svelte';

export type TransformOp = 'reverse' | 'trim' | 'normalize' | 'gain' | 'fade';

export interface TransformMeta {
	name: string;
	filePath: string;
	hash: string;
	durationMs: number;
	sampleRate: number;
	channels: number;
	fileSize: number;
	sourcePath: string;
	operation: string;
	parametersJson: string;
}

export interface DirtGroup {
	name: string;
	files: string[];
}

type TransformEvent =
	| { kind: 'progress'; phase: string }
	| { kind: 'done'; meta: TransformMeta }
	| { kind: 'error'; message: string };

type Spec =
	| { op: 'reverse' }
	| { op: 'trim'; startMs: number; endMs: number }
	| { op: 'normalize'; mode: 'peak' | 'rms' }
	| { op: 'gain'; db: number }
	| { op: 'fade'; inMs: number; outMs: number };

const AUDIO_FILTERS = [
	{ name: 'Audio', extensions: ['wav', 'aif', 'aiff', 'aifc', 'flac', 'mp3', 'ogg', 'caf'] }
];

export const transform = $state({
	open: false,
	sourcePath: '',
	op: 'reverse' as TransformOp,
	startMs: 0,
	endMs: 0,
	mode: 'peak' as 'peak' | 'rms',
	db: 0,
	inMs: 0,
	outMs: 0,
	bitsPerSample: 16 as 16 | 24,
	running: false,
	phase: '',
	error: '',
	lastMeta: null as TransformMeta | null,
	dirtGroups: [] as DirtGroup[],
	selectedGroup: '',
	loadingGroups: false
});

function buildSpec(): Spec {
	switch (transform.op) {
		case 'reverse':
			return { op: 'reverse' };
		case 'trim':
			return { op: 'trim', startMs: transform.startMs, endMs: transform.endMs };
		case 'normalize':
			return { op: 'normalize', mode: transform.mode };
		case 'gain':
			return { op: 'gain', db: transform.db };
		case 'fade':
			return { op: 'fade', inMs: transform.inMs, outMs: transform.outMs };
	}
}

export function selectedGroupFiles(): string[] {
	const group = transform.dirtGroups.find((g) => g.name === transform.selectedGroup);
	return group?.files ?? [];
}

export async function loadDirtGroups() {
	if (transform.dirtGroups.length > 0 || transform.loadingGroups) return;
	transform.loadingGroups = true;
	try {
		transform.dirtGroups = await invoke<DirtGroup[]>('dirt_samples_groups');
	} catch (e) {
		transform.error = String(e);
	} finally {
		transform.loadingGroups = false;
	}
}

export function selectGroup(name: string) {
	transform.selectedGroup = name;
	const group = transform.dirtGroups.find((g) => g.name === name);
	transform.sourcePath = group?.files[0] ?? '';
}

export async function pickSourceFile() {
	const picked = await open({ multiple: false, filters: AUDIO_FILTERS });
	if (typeof picked !== 'string') return;
	transform.sourcePath = picked;
	transform.selectedGroup = '';
}

let unlisten: UnlistenFn | undefined;

async function ensureProgressListener() {
	if (unlisten) return;
	unlisten = await listen<TransformEvent>('transform://event', (e) => {
		if (e.payload.kind === 'progress') transform.phase = e.payload.phase;
	});
}

async function registerDerived(meta: TransformMeta, spec: Spec) {
	const d = await getDb();
	const now = new Date().toISOString();
	const params = JSON.stringify({ ...spec, sourcePath: meta.sourcePath });
	const rows = await d.select<{ id: number }[]>(
		`INSERT INTO samples
		   (name, file_path, hash, source_type, parent_id, duration_ms, sample_rate, channels, file_size, is_stale, created_at)
		 VALUES ($1, $2, $3, 'derived', NULL, $4, $5, $6, $7, 0, $8)
		 ON CONFLICT(file_path) DO UPDATE SET
		   hash = $3, duration_ms = $4, sample_rate = $5, channels = $6, file_size = $7, created_at = $8
		 RETURNING id`,
		[
			meta.name,
			meta.filePath,
			meta.hash,
			meta.durationMs,
			meta.sampleRate,
			meta.channels,
			meta.fileSize,
			now
		]
	);
	const id = rows[0]?.id;
	if (id === undefined) {
		session.notify(`could not register ${meta.name} (no row id returned)`);
		return;
	}
	await d.execute(
		`INSERT INTO sample_transformations (sample_id, result_sample_id, operation_type, parameters_json, created_at)
		 VALUES ($1, $1, $2, $3, $4)`,
		[id, meta.operation, params, now]
	);
}

export async function applyTransform() {
	if (transform.running) return;
	if (!transform.sourcePath) {
		transform.error = 'pick a source file first';
		return;
	}
	transform.running = true;
	transform.error = '';
	transform.phase = 'decode';
	transform.lastMeta = null;
	try {
		await ensureProgressListener();
		const meta = await invoke<TransformMeta>('sample_transform', {
			sourcePath: transform.sourcePath,
			spec: buildSpec(),
			bitsPerSample: transform.bitsPerSample
		});
		transform.lastMeta = meta;
		await registerDerived(meta, buildSpec());
		await invoke('sample_reload');
		session.notify(`derived ${meta.name} ready — press preview`);
	} catch (e) {
		transform.error = String(e);
	} finally {
		transform.running = false;
		transform.phase = '';
	}
}

export function previewLast() {
	if (!transform.lastMeta) return;
	const code = `once $ s "${transform.lastMeta.name}"`;
	session.echo(code);
	void session.eval(code);
}

export async function copyName() {
	if (!transform.lastMeta) return;
	try {
		await navigator.clipboard.writeText(transform.lastMeta.name);
		session.notify('sample name copied');
	} catch {
		session.notify(transform.lastMeta.name);
	}
}
