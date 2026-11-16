import type {
	Completion,
	CompletionContext,
	CompletionResult,
	CompletionSource
} from '@codemirror/autocomplete';
import { insideSoundLiteral } from '../fragments';
import { formatDuration, library } from '../library.svelte';
import { loadDirtGroups, transform } from '../transform.svelte';

export interface SampleCompletion extends Completion {
	sample?: string;
}

export type SoundSource = 'custom' | 'derived' | 'dirt-samples' | 'builtin';

export interface SoundEntry {
	name: string;
	source: SoundSource;
	durationMs?: number | null;
	filePath?: string;
}

const BUILTIN: string[] = ['bd', 'sn', 'hh', 'lt', 'ht', 'cp', 'casio', 'can'];

const PLAY_SHORTCUT = 'Ctrl+P';

const SAMPLE_WORD = /[A-Za-z0-9_-]*$/;
const MODIFIER_BEFORE = /[:*!?@^]$/;

export function sampleCompletion(preview: (name: string) => void): CompletionSource {
	return (cx: CompletionContext): CompletionResult | null => {
		const line = cx.state.doc.lineAt(cx.pos);
		const before = line.text.slice(0, cx.pos - line.from);
		if (!insideSoundLiteral(before)) return null;

		const word = cx.matchBefore(SAMPLE_WORD);
		if (!word) return null;
		const from = word.from;
		if (from > line.from && MODIFIER_BEFORE.test(cx.state.sliceDoc(from - 1, from))) return null;
		void loadDirtGroups();

		const prefix = word.text.toLowerCase();
		const options: SampleCompletion[] = candidates()
			.filter((entry) => entry.name.toLowerCase().startsWith(prefix))
			.slice(0, 80)
			.map((entry) => ({
				label: entry.name,
				type: 'text',
				detail: detailOf(entry),
				sample: entry.name,
				info: () => infoNode(entry, preview)
			}));
		if (options.length === 0) return null;
		return { from, options };
	};
}

function candidates(): SoundEntry[] {
	const seen = new Set<string>();
	const out: SoundEntry[] = [];
	for (const sample of library.samples) {
		if (seen.has(sample.name)) continue;
		seen.add(sample.name);
		out.push({
			name: sample.name,
			source: sample.sourceType,
			durationMs: sample.durationMs,
			filePath: sample.filePath
		});
	}
	for (const group of transform.dirtGroups) {
		if (seen.has(group.name)) continue;
		seen.add(group.name);
		out.push({ name: group.name, source: 'dirt-samples' });
	}
	for (const name of BUILTIN) {
		if (seen.has(name)) continue;
		seen.add(name);
		out.push({ name, source: 'builtin' });
	}
	return out;
}

function detailOf(entry: SoundEntry): string {
	if (entry.durationMs !== undefined && entry.durationMs !== null) {
		return `${formatDuration(entry.durationMs)} · ${entry.source}`;
	}
	return entry.source;
}

function infoNode(entry: SoundEntry, preview: (name: string) => void): HTMLElement {
	const root = document.createElement('div');
	root.className = 'cm-sample-info';

	const title = document.createElement('div');
	title.textContent =
		entry.durationMs !== undefined && entry.durationMs !== null
			? `${entry.name} — ${formatDuration(entry.durationMs)} · ${entry.source}`
			: `${entry.name} — ${entry.source}`;
	root.appendChild(title);

	if (entry.filePath) {
		const path = document.createElement('div');
		path.className = 'cm-sample-path';
		path.textContent = entry.filePath;
		root.appendChild(path);
	}

	const play = document.createElement('button');
	play.type = 'button';
	play.className = 'cm-sample-play';
	play.textContent = '▶ play';
	play.title = `once $ s "${entry.name}" — ${PLAY_SHORTCUT}`;
	play.addEventListener('mousedown', (event) => event.preventDefault());
	play.addEventListener('click', () => preview(entry.name));
	root.appendChild(play);

	return root;
}
