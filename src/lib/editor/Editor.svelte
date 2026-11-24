<script lang="ts">
	import { EditorState, Prec, StateEffect, StateField } from '@codemirror/state';
	import { Decoration, type DecorationSet, EditorView, keymap, WidgetType } from '@codemirror/view';
	import { HighlightStyle, LanguageSupport, syntaxHighlighting } from '@codemirror/language';
	import { tags } from '@lezer/highlight';
	import {
		acceptCompletion,
		autocompletion,
		completeAnyWord,
		completionStatus,
		currentCompletions,
		selectedCompletionIndex
	} from '@codemirror/autocomplete';
	import { indentLess, indentMore } from '@codemirror/commands';
	import { basicSetup } from 'codemirror';
	import { untrack } from 'svelte';
	import type { SessionDiagnostic } from '$lib/session.svelte';
	import { sampleCompletion, type SampleCompletion } from './samples';
	import { tidalCompletion } from './vocabulary';
	import { tidalLanguage } from './tidal-mode';

	interface FlashRange {
		from: number;
		to: number;
		seq: number;
		durationMs?: number;
	}

	interface DiagnosticWidgetEntry {
		key: string;
		diagnostic: SessionDiagnostic;
		expanded: boolean;
	}

	class DiagnosticWidget extends WidgetType {
		private readonly signature: string;
		private readonly entries: DiagnosticWidgetEntry[];
		private readonly onToggle: (key: string) => void;

		constructor(entries: DiagnosticWidgetEntry[], onToggle: (key: string) => void) {
			super();
			this.entries = entries;
			this.onToggle = onToggle;
			this.signature = entries
				.map(
					({ key, diagnostic, expanded }) =>
						`${key}\u0000${diagnostic.summary}\u0000${diagnostic.details.join('\n')}\u0000${expanded}`
				)
				.join('\u0001');
		}

		eq(other: WidgetType): boolean {
			return other instanceof DiagnosticWidget && this.signature === other.signature;
		}

		toDOM(): HTMLElement {
			const root = document.createElement('div');
			root.className = 'cm-diagnostic-widget';
			root.contentEditable = 'false';
			root.setAttribute('role', 'group');
			root.setAttribute('aria-label', 'errors for this line');

			for (const entry of this.entries) {
				const row = document.createElement('div');
				row.className = 'cm-diagnostic-entry';

				const header = document.createElement('div');
				header.className = 'cm-diagnostic-header';
				const summary = document.createElement('span');
				summary.className = 'cm-diagnostic-summary';
				summary.textContent = entry.diagnostic.summary;
				header.append(summary);

				if (entry.diagnostic.details.length > 0) {
					const button = document.createElement('button');
					button.type = 'button';
					button.className = 'cm-diagnostic-toggle';
					button.setAttribute('aria-expanded', String(entry.expanded));
					button.textContent = entry.expanded ? 'hide details' : 'show details';
					button.addEventListener('click', (event) => {
						event.preventDefault();
						event.stopPropagation();
						this.onToggle(entry.key);
					});
					header.append(button);
				}
				row.append(header);

				if (entry.expanded && entry.diagnostic.details.length > 0) {
					const details = document.createElement('pre');
					details.className = 'cm-diagnostic-details';
					details.textContent = entry.diagnostic.details.join('\n');
					row.append(details);
				}

				root.append(row);
			}

			return root;
		}

		ignoreEvent(): boolean {
			return true;
		}
	}

	function buildDiagnosticDecorations(
		state: EditorState,
		diagnostics: SessionDiagnostic[],
		expandedKeys: string[],
		onToggle: (key: string) => void
	): DecorationSet {
		const grouped: { lineNumber: number; entries: DiagnosticWidgetEntry[] }[] = [];
		for (const diagnostic of diagnostics) {
			if (diagnostic.line < 1 || diagnostic.line > state.doc.lines) continue;
			let group = grouped.find((item) => item.lineNumber === diagnostic.line);
			if (!group) {
				group = { lineNumber: diagnostic.line, entries: [] };
				grouped.push(group);
			}
			const entries = group.entries;
			const key = [
				diagnostic.requestId,
				diagnostic.statementIndex ?? 'single',
				diagnostic.line,
				diagnostic.column ?? '',
				entries.length
			].join(':');
			entries.push({ key, diagnostic, expanded: expandedKeys.includes(key) });
		}

		const decorations = [];
		for (const { lineNumber, entries } of grouped) {
			const line = state.doc.line(lineNumber);
			decorations.push(Decoration.line({ class: 'cm-error-line' }).range(line.from));
			decorations.push(
				Decoration.widget({
					widget: new DiagnosticWidget(entries, onToggle),
					block: true,
					side: 1
				}).range(line.to)
			);
		}
		return Decoration.set(decorations, true);
	}

	function isContinuationLine(text: string): boolean {
		const trimmed = text.trim();
		return !trimmed || /^[ \t]/.test(text) || trimmed.startsWith('--');
	}

	function insideBlockComment(text: string): boolean {
		const closers: string[] = [];
		let inString = false;
		for (let i = 0; i < text.length; i += 1) {
			if (inString) {
				if (text[i] === '\\') i += 1;
				else if (text[i] === '"') inString = false;
				continue;
			}

			if (closers.length > 0) {
				if (text.startsWith('{--', i)) {
					closers.push('--}');
					i += 2;
				} else if (text.startsWith('{-', i)) {
					closers.push('-}');
					i += 1;
				} else if (text.startsWith(closers.at(-1)!, i)) {
					i += closers.pop()!.length - 1;
				}
				continue;
			}

			if (text.startsWith('--', i)) {
				const newline = text.indexOf('\n', i + 2);
				if (newline < 0) break;
				i = newline;
			} else if (text.startsWith('{--', i)) {
				closers.push('--}');
				i += 2;
			} else if (text.startsWith('{-', i)) {
				closers.push('-}');
				i += 1;
			} else if (text[i] === '"') {
				inString = true;
			}
		}
		return closers.length > 0;
	}

	function insideGhciBlock(state: EditorState, line: number): boolean {
		let open = false;
		for (let number = 1; number <= line; number += 1) {
			const text = state.doc.line(number).text.trim();
			if (text.startsWith(':{')) open = true;
			else if (text === ':}') open = false;
		}
		return open;
	}

	function statementAt(
		state: EditorState,
		position: number
	): { code: string; line: number } | null {
		if (insideBlockComment(state.doc.sliceString(0, position))) return null;
		const current = state.doc.lineAt(position);
		if (insideGhciBlock(state, current.number)) return null;
		const trimmed = current.text.trim();
		if (
			!trimmed ||
			trimmed.startsWith('--') ||
			trimmed.startsWith('{-') ||
			trimmed.startsWith(':')
		) {
			return null;
		}

		let first = current.number;
		while (first > 1 && isContinuationLine(state.doc.line(first - 1).text)) first -= 1;

		let last = current.number;
		while (last < state.doc.lines && isContinuationLine(state.doc.line(last + 1).text)) last += 1;

		const from = state.doc.line(first).from;
		const to = state.doc.line(last).to;
		return { code: state.doc.sliceString(from, to), line: first };
	}

	interface Props {
		content?: string;
		diagnostics?: SessionDiagnostic[];
		flashRanges?: FlashRange[];
		onEval?: (code: string, line: number) => void;
		onRunAll?: (code: string) => void;
		onHush?: () => void;
		onPreviewSample?: (name: string) => void;
		onDocumentChange?: () => void;
		onCheck?: (code: string, line: number) => void;
	}

	let {
		content = $bindable(''),
		diagnostics = [],
		flashRanges = [],
		onEval = () => {},
		onRunAll = () => {},
		onHush = () => {},
		onPreviewSample = () => {},
		onDocumentChange = () => {},
		onCheck = () => {}
	}: Props = $props();

	let host: HTMLDivElement | undefined = $state();
	let view: EditorView | undefined;
	let expandedDiagnosticKeys = $state<string[]>([]);
	let expandedDiagnosticRequestId: string | null = null;
	let typeCheckTimer: ReturnType<typeof setTimeout> | undefined;

	const samples = sampleCompletion((name) => onPreviewSample(name));

	function previewSelectedSample(v: EditorView): boolean {
		if (completionStatus(v.state) !== 'active') return false;
		const options = currentCompletions(v.state) as readonly SampleCompletion[];
		const index = selectedCompletionIndex(v.state);
		const chosen = index === null ? options[0] : options[index];
		if (!chosen?.sample) return false;
		onPreviewSample(chosen.sample);
		return true;
	}

	const tidalSupport = new LanguageSupport(tidalLanguage, [
		autocompletion({ override: [samples, tidalCompletion, completeAnyWord] }),
		Prec.high(
			keymap.of([
				{ key: 'Tab', run: (v) => acceptCompletion(v) || indentMore(v) },
				{ key: 'Shift-Tab', run: indentLess }
			])
		)
	]);

	const synthwaveTheme = EditorView.theme(
		{
			'&': { backgroundColor: '#000000', color: '#36f9f6' },
			'.cm-content': { color: '#36f9f6', caretColor: '#fede5d' },
			'.cm-cursor, .cm-dropCursor': { borderLeftColor: '#fede5d' },
			'.cm-selectionBackground, .cm-content ::selection': {
				backgroundColor: '#001069'
			},
			'&.cm-focused .cm-selectionBackground, .cm-content ::selection': {
				backgroundColor: '#001069'
			},
			'.cm-activeLine': { backgroundColor: 'rgba(255, 255, 255, 0.04)' },
			'.cm-activeLineGutter': { backgroundColor: '#141414', color: '#ffffff' }
		},
		{ dark: true }
	);

	const synthwaveHighlight = HighlightStyle.define([
		{ tag: tags.comment, color: '#848bbd', fontStyle: 'italic' },
		{ tag: [tags.keyword, tags.operatorKeyword], color: '#fede5d' },
		{ tag: [tags.operator, tags.punctuation, tags.bracket], color: '#fede5d' },
		{ tag: tags.number, color: '#ff5252' },
		{ tag: tags.string, color: '#28b881' },
		{ tag: tags.typeName, color: '#fe4450' },
		{ tag: tags.variableName, color: '#f73232' },
		{ tag: tags.function(tags.variableName), color: '#36f9f6' },
		{ tag: tags.definition(tags.variableName), color: '#36f9f6' },
		{ tag: [tags.bool, tags.null], color: '#1afc65' },
		{ tag: tags.meta, color: '#848bbd' },
		{ tag: tags.invalid, color: '#ff5252' }
	]);

	function lineOrSelection(v: EditorView): { code: string; line: number } {
		const { from, to } = v.state.selection.main;
		const line = v.state.doc.lineAt(from).number;
		if (from !== to) return { code: v.state.sliceDoc(from, to), line };
		return { code: v.state.doc.lineAt(from).text, line };
	}

	function evalLineOrSelection(v: EditorView): boolean {
		cancelTypeCheck();
		const { code, line } = lineOrSelection(v);
		if (code.trim()) onEval(code, line);
		return true;
	}

	function cancelTypeCheck() {
		if (typeCheckTimer !== undefined) clearTimeout(typeCheckTimer);
		typeCheckTimer = undefined;
	}

	export function cancelPendingTypeCheck() {
		cancelTypeCheck();
	}

	function scheduleTypeCheck(v: EditorView) {
		cancelTypeCheck();
		const statement = statementAt(v.state, v.state.selection.main.head);
		if (!statement) return;
		typeCheckTimer = setTimeout(() => {
			typeCheckTimer = undefined;
			if (view === v) onCheck(statement.code, statement.line);
		}, 600);
	}

	const setDiagnostics = StateEffect.define<{
		diagnostics: SessionDiagnostic[];
		expandedKeys: string[];
	}>();

	const diagnosticField = StateField.define<DecorationSet>({
		create: () => Decoration.none,
		update(decorations, tr) {
			let next = tr.docChanged ? Decoration.none : decorations.map(tr.changes);
			for (const effect of tr.effects) {
				if (!effect.is(setDiagnostics)) continue;
				next = buildDiagnosticDecorations(
					tr.state,
					effect.value.diagnostics,
					effect.value.expandedKeys,
					toggleDiagnostic
				);
			}
			return next;
		},
		provide: (field) => EditorView.decorations.from(field)
	});

	const setFlash = StateEffect.define<FlashRange[] | null>();
	const flashMark = (seq: number, durationMs?: number) => {
		const attributes = durationMs ? { style: `animation-duration: ${durationMs}ms` } : undefined;
		return Decoration.mark({
			class: seq % 2 ? 'cm-flash cm-flash-alt' : 'cm-flash',
			attributes
		});
	};

	const flashField = StateField.define<DecorationSet>({
		create: () => Decoration.none,
		update(decorations, tr) {
			let next = decorations.map(tr.changes);
			for (const effect of tr.effects) {
				if (!effect.is(setFlash)) continue;
				if (!effect.value || effect.value.length === 0) return Decoration.none;
				const docLength = tr.state.doc.length;
				const ranges = effect.value
					.map((range) => ({
						...range,
						from: Math.min(Math.max(range.from, 0), docLength),
						to: Math.min(Math.max(range.to, 0), docLength)
					}))
					.filter((range) => range.to > range.from);
				if (ranges.length === 0) return Decoration.none;
				next = Decoration.set(
					ranges.map((range) => flashMark(range.seq, range.durationMs).range(range.from, range.to)),
					true
				);
			}
			return next;
		},
		provide: (field) => EditorView.decorations.from(field)
	});

	$effect(() => {
		if (!host) return;
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: untrack(() => content),
				extensions: [
					basicSetup,
					tidalSupport,
					synthwaveTheme,
					syntaxHighlighting(synthwaveHighlight),
					EditorView.lineWrapping,
					diagnosticField,
					flashField,
					Prec.high(
						keymap.of([
							{
								key: 'Shift-Mod-Enter',
								preventDefault: true,
								run: (v) => {
									cancelTypeCheck();
									onRunAll(v.state.doc.toString());
									return true;
								}
							},
							{
								key: 'Mod-Enter',
								preventDefault: true,
								run: evalLineOrSelection
							},
							{
								key: 'Ctrl-Enter',
								preventDefault: true,
								run: evalLineOrSelection
							},
							{
								key: 'Ctrl-p',
								run: previewSelectedSample
							},
							{
								key: 'Alt-Enter',
								run: previewSelectedSample
							},
							{
								key: 'ArrowRight',
								run: (v) => (completionStatus(v.state) === 'active' ? acceptCompletion(v) : false)
							},
							{
								key: 'Mod-.',
								preventDefault: true,
								run: () => {
									onHush();
									return true;
								}
							}
						])
					),
					EditorView.updateListener.of((u) => {
						if (u.docChanged) {
							content = u.state.doc.toString();
							onDocumentChange();
							scheduleTypeCheck(u.view);
						}
					})
				]
			})
		});
		return () => {
			cancelTypeCheck();
			view?.destroy();
			view = undefined;
		};
	});

	function toggleDiagnostic(key: string) {
		expandedDiagnosticKeys = expandedDiagnosticKeys.includes(key)
			? expandedDiagnosticKeys.filter((current) => current !== key)
			: [...expandedDiagnosticKeys, key];
	}

	$effect(() => {
		const requestId = diagnostics[0]?.requestId ?? null;
		if (requestId !== expandedDiagnosticRequestId) {
			expandedDiagnosticRequestId = requestId;
			expandedDiagnosticKeys = [];
		}
	});

	$effect(() => {
		if (!view) return;
		view.dispatch({
			effects: setDiagnostics.of({ diagnostics, expandedKeys: expandedDiagnosticKeys })
		});
	});

	$effect(() => {
		if (!view) return;
		view.dispatch({ effects: setFlash.of(flashRanges.length ? flashRanges : null) });
	});

	$effect(() => {
		if (!view) return;
		const current = view.state.doc.toString();
		if (content !== current) {
			view.dispatch({ changes: { from: 0, to: current.length, insert: content } });
		}
	});

	export function selection(): string {
		const v = view;
		if (!v) return '';
		const { from, to } = v.state.selection.main;
		return from === to ? '' : v.state.sliceDoc(from, to);
	}

	export function insert(code: string): boolean {
		const v = view;
		if (!v) return false;
		const { from, to } = v.state.selection.main;
		v.dispatch({
			changes: { from, to, insert: code },
			selection: { anchor: from + code.length },
			scrollIntoView: true
		});
		v.focus();
		return true;
	}
</script>

<div class="editor-host" bind:this={host}></div>

<style>
	.editor-host {
		height: 100%;
		overflow: hidden;
	}

	:global(.editor-host .cm-editor) {
		height: 100%;
		font-size: 14px;
		background: var(--sw-bg);
		color: var(--sw-blue);
	}

	:global(.editor-host .cm-gutters) {
		background: var(--sw-gutter);
		color: var(--sw-grey);
		border-right: 1px solid var(--sw-border);
	}

	:global(.editor-host .cm-scroller) {
		font-family: 'JetBrains Mono', Menlo, Monaco, Consolas, monospace;
	}

	:global(.editor-host .cm-error-line) {
		background: color-mix(in srgb, var(--sw-red) 15%, transparent);
	}

	:global(.editor-host .cm-diagnostic-widget) {
		display: block;
		box-sizing: border-box;
		width: 100%;
		margin: 0.12rem 0 0.3rem;
		padding: 0.3rem 0.6rem;
		border-left: 2px solid var(--sw-red);
		background: color-mix(in srgb, var(--sw-red) 8%, var(--sw-bg));
		color: var(--sw-text);
		font-size: 0.78rem;
		line-height: 1.45;
		white-space: normal;
	}

	:global(.editor-host .cm-diagnostic-entry + .cm-diagnostic-entry) {
		margin-top: 0.35rem;
		padding-top: 0.35rem;
		border-top: 1px solid color-mix(in srgb, var(--sw-red) 25%, transparent);
	}

	:global(.editor-host .cm-diagnostic-header) {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 0.75rem;
	}

	:global(.editor-host .cm-diagnostic-summary) {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	:global(.editor-host .cm-diagnostic-toggle) {
		flex: 0 0 auto;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--sw-muted);
		font: inherit;
		font-size: 0.68rem;
		text-decoration: underline;
		text-underline-offset: 0.15em;
		cursor: pointer;
	}

	:global(.editor-host .cm-diagnostic-toggle:hover),
	:global(.editor-host .cm-diagnostic-toggle:focus-visible) {
		color: var(--sw-text);
	}

	:global(.editor-host .cm-diagnostic-toggle:focus-visible) {
		outline: 1px solid var(--sw-red);
		outline-offset: 2px;
	}

	:global(.editor-host .cm-diagnostic-details) {
		max-width: 100%;
		margin: 0.3rem 0 0;
		color: var(--sw-muted);
		font: inherit;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	:global(.cm-tooltip.cm-tooltip-autocomplete) {
		background: var(--sw-surface);
		color: var(--sw-text);
		border: 1px solid var(--sw-border);
	}

	:global(.cm-tooltip-autocomplete > ul > li[aria-selected]) {
		background: var(--sw-selection);
		color: var(--sw-text);
	}

	:global(.cm-tooltip.cm-completionInfo) {
		background: var(--sw-surface);
		color: var(--sw-text);
		border: 1px solid var(--sw-border);
	}

	:global(.cm-completionDetail) {
		color: var(--sw-muted);
		font-style: italic;
	}

	:global(.cm-sample-info) {
		font-size: 0.72rem;
		max-width: 22rem;
	}

	:global(.cm-sample-path) {
		color: var(--sw-muted);
		font-size: 0.65rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	:global(.cm-sample-play) {
		margin-top: 0.3rem;
		font: inherit;
		font-size: 0.7rem;
		padding: 0.1rem 0.45rem;
		border: 1px solid var(--sw-border);
		border-radius: 4px;
		background: transparent;
		color: var(--sw-blue);
		cursor: pointer;
	}

	:global(.cm-sample-play:hover) {
		background: color-mix(in srgb, var(--sw-blue) 15%, transparent);
	}
</style>
