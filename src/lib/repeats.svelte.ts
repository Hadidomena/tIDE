import { invoke } from '@tauri-apps/api/core';
import { getDb, getSetting } from './db';
import { project } from './project.svelte';

export interface RepeatSpan {
	startLine: number;
	endLine: number;
	startByte: number;
	endByte: number;
}

export interface Repeat {
	hash: string;
	code: string;
	tokenLen: number;
	occurrences: number;
	spans: RepeatSpan[];
}

export interface RepeatItem extends Repeat {
	key: string;
}

export interface RepeatFocus {
	fromLine: number;
	toLine: number;
	seq: number;
}

export const repeats = $state({
	open: false,
	items: [] as RepeatItem[],
	analyzing: false,
	error: '',
	selectedKey: '',
	focus: null as RepeatFocus | null
});

let generation = 0;
let debounce: ReturnType<typeof setTimeout> | undefined;
let focusSeq = 0;

async function ngramRange(): Promise<{ min: number; max: number }> {
	const min = Number.parseInt((await getSetting('ngram_min').catch(() => null)) ?? '', 10);
	const max = Number.parseInt((await getSetting('ngram_max').catch(() => null)) ?? '', 10);
	return {
		min: Number.isFinite(min) ? min : 3,
		max: Number.isFinite(max) ? max : 64
	};
}

export async function analyze(code: string) {
	const gen = ++generation;
	repeats.analyzing = true;
	repeats.error = '';
	try {
		const { min, max } = await ngramRange();
		const result = await invoke<Repeat[]>('analyze_patterns', {
			code,
			minTokens: min,
			maxTokens: max
		});
		if (gen !== generation) return;
		repeats.items = result.map((r, i) => ({ ...r, key: `${r.hash}-${i}` }));
		await persist(repeats.items);
	} catch (e) {
		if (gen === generation) {
			repeats.error = String(e);
			repeats.items = [];
		}
	} finally {
		if (gen === generation) repeats.analyzing = false;
	}
}

export function scheduleAnalyze(code: string, delayMs = 600) {
	if (debounce) clearTimeout(debounce);
	debounce = setTimeout(() => {
		debounce = undefined;
		void analyze(code);
	}, delayMs);
}

async function persist(items: RepeatItem[]) {
	if (project.id === null) return;
	try {
		const db = await getDb();
		const now = new Date().toISOString();
		await db.execute('DELETE FROM pattern_occurrences WHERE project_id = $1', [project.id]);
		for (const item of items) {
			const first = item.spans[0];
			await db.execute(
				`INSERT INTO pattern_occurrences
				   (project_id, path_hash, code, file_path, start_line, end_line, occurrences, analyzed_at)
				 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)`,
				[
					project.id,
					item.hash,
					item.code,
					project.path,
					first?.startLine ?? null,
					first?.endLine ?? null,
					item.occurrences,
					now
				]
			);
		}
	} catch (e) {
		repeats.error = String(e);
	}
}

export function selectRepeat(key: string) {
	const item = repeats.items.find((r) => r.key === key);
	repeats.selectedKey = key;
	const span = item?.spans[0];
	repeats.focus = span ? { fromLine: span.startLine, toLine: span.endLine, seq: ++focusSeq } : null;
}

export function clearRepeats() {
	if (debounce) {
		clearTimeout(debounce);
		debounce = undefined;
	}
	generation += 1;
	repeats.items = [];
	repeats.selectedKey = '';
	repeats.focus = null;
	repeats.analyzing = false;
}
