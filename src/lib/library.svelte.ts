import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { getDb } from './db';
import { session } from './session.svelte';
import { transform } from './transform.svelte';

export type LibraryCategory = 'samples' | 'patterns';

export type SampleSource = 'custom' | 'derived';

export interface LibrarySample {
	id: number;
	name: string;
	filePath: string;
	sourceType: SampleSource;
	durationMs: number | null;
	sampleRate: number | null;
	channels: number | null;
}

export interface LibraryPattern {
	id: number;
	title: string;
	codeContent: string;
	useCount: number;
	updatedAt: string;
}

export interface ImportedSample {
	name: string;
	filePath: string;
	hash: string;
	durationMs: number;
	sampleRate: number;
	channels: number;
	fileSize: number;
}

const AUDIO_FILTERS = [
	{ name: 'Audio', extensions: ['wav', 'aif', 'aiff', 'aifc', 'flac', 'mp3', 'ogg', 'caf'] }
];

function isoNow(): string {
	return new Date().toISOString();
}

export const library = $state({
	open: false,
	category: 'samples' as LibraryCategory,
	samples: [] as LibrarySample[],
	patterns: [] as LibraryPattern[],
	loading: false,
	importing: false,
	deletingId: null as number | null,
	error: ''
});

export function openLibrary(category: LibraryCategory = 'samples') {
	library.open = true;
	library.category = category;
	transform.open = false;
	void refresh();
}

export function toggleLibrary() {
	if (library.open) {
		library.open = false;
		return;
	}
	openLibrary(library.category);
}

export async function refresh() {
	library.loading = true;
	library.error = '';
	try {
		const d = await getDb();
		library.samples = await d.select<LibrarySample[]>(
			`SELECT id, name, file_path AS filePath, source_type AS sourceType,
			        duration_ms AS durationMs, sample_rate AS sampleRate, channels
			   FROM samples
			  WHERE source_type IN ('custom', 'derived')
			  ORDER BY source_type, name`
		);
		library.patterns = await d.select<LibraryPattern[]>(
			`SELECT id, title, code_content AS codeContent,
			        use_count AS useCount, updated_at AS updatedAt
			   FROM snippets
			  WHERE kind = 'pattern'
			  ORDER BY use_count DESC, updated_at DESC`
		);
	} catch (e) {
		library.error = String(e);
	} finally {
		library.loading = false;
	}
}

export async function importSamples() {
	if (library.importing) return;
	let picked: string | string[] | null;
	try {
		picked = await open({ multiple: true, filters: AUDIO_FILTERS });
	} catch (e) {
		library.error = String(e);
		return;
	}
	if (typeof picked === 'string') picked = [picked];
	if (!picked || picked.length === 0) return;

	library.importing = true;
	library.error = '';
	try {
		const imported = await invoke<ImportedSample[]>('sample_import', { paths: picked });
		const d = await getDb();
		const now = isoNow();
		let added = 0;
		let skipped = 0;
		for (const sample of imported) {
			const dupe = await d.select<{ id: number }[]>(
				'SELECT id FROM samples WHERE hash = $1 LIMIT 1',
				[sample.hash]
			);
			if (dupe.length > 0) {
				await invoke('sample_delete', { name: sample.name, sourceType: 'custom' });
				skipped += 1;
				continue;
			}
			await d.execute(
				`INSERT INTO samples
				   (name, file_path, hash, source_type, parent_id, duration_ms, sample_rate, channels, file_size, is_stale, created_at)
				 VALUES ($1, $2, $3, 'custom', NULL, $4, $5, $6, $7, 0, $8)
				 ON CONFLICT(file_path) DO UPDATE SET
				   hash = $3, duration_ms = $4, sample_rate = $5, channels = $6, file_size = $7`,
				[
					sample.name,
					sample.filePath,
					sample.hash,
					sample.durationMs,
					sample.sampleRate,
					sample.channels,
					sample.fileSize,
					now
				]
			);
			added += 1;
		}
		if (added > 0) await invoke('sample_reload');
		session.notify(
			`imported ${added} sample${added === 1 ? '' : 's'}` +
				(skipped > 0 ? `, skipped ${skipped} already in the library` : '')
		);
		await refresh();
	} catch (e) {
		library.error = String(e);
	} finally {
		library.importing = false;
	}
}

export function previewSample(name: string) {
	const code = `once $ s "${name}"`;
	session.echo(code);
	void session.eval(code);
}

export async function deleteSample(sample: LibrarySample) {
	if (library.deletingId !== null) return;
	library.deletingId = sample.id;
	library.error = '';
	try {
		await invoke('sample_delete', { name: sample.name, sourceType: sample.sourceType });
		const d = await getDb();
		await d.execute('DELETE FROM samples WHERE id = $1', [sample.id]);
		await refresh();
		session.notify(
			`deleted ${sample.name} — a running engine keeps its buffers until the next reload`
		);
	} catch (e) {
		library.error = String(e);
	} finally {
		library.deletingId = null;
	}
}

export function transformSample(sample: LibrarySample) {
	transform.sourcePath = sample.filePath;
	transform.selectedGroup = '';
	transform.lastMeta = null;
	library.open = false;
	transform.open = true;
}

export function formatDuration(ms: number | null): string {
	if (ms === null || !Number.isFinite(ms)) return '?';
	if (ms < 1000) return `${ms} ms`;
	return `${(ms / 1000).toFixed(2)} s`;
}
