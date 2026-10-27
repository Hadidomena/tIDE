<script lang="ts">
	import { EditorState, Prec, StateEffect, StateField } from '@codemirror/state';
	import { Decoration, type DecorationSet, EditorView, keymap } from '@codemirror/view';
	import { HighlightStyle, LanguageSupport, syntaxHighlighting } from '@codemirror/language';
	import { tags } from '@lezer/highlight';
	import { acceptCompletion, autocompletion, completeAnyWord } from '@codemirror/autocomplete';
	import { indentLess, indentMore } from '@codemirror/commands';
	import { basicSetup } from 'codemirror';
	import { untrack } from 'svelte';
	import { tidalCompletion } from './vocabulary';
	import { tidalLanguage } from './tidal-mode';

	interface Props {
		content?: string;
		errorLine?: number | null;
		onEval?: (code: string, line: number) => void;
		onRunAll?: (code: string) => void;
		onHush?: () => void;
	}

	let {
		content = $bindable(''),
		errorLine = null,
		onEval = () => {},
		onRunAll = () => {},
		onHush = () => {}
	}: Props = $props();

	let host: HTMLDivElement | undefined = $state();
	let view: EditorView | undefined;

	const tidalSupport = new LanguageSupport(tidalLanguage, [
		autocompletion({ override: [tidalCompletion, completeAnyWord] }),
		Prec.high(
			keymap.of([
				{ key: 'Tab', run: (v) => acceptCompletion(v) || indentMore(v) },
				{ key: 'Shift-Tab', run: indentLess }
			])
		)
	]);

	const synthwaveTheme = EditorView.theme(
		{
			'&': { backgroundColor: '#000000', color: '#ffffff' },
			'.cm-content': { caretColor: '#fede5d' },
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
		{ tag: tags.number, color: '#f97e72' },
		{ tag: tags.string, color: '#ff8b39' },
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
		const { code, line } = lineOrSelection(v);
		if (code.trim()) onEval(code, line);
		return true;
	}

	const setErrorLine = StateEffect.define<number | null>();

	const errorLineField = StateField.define<DecorationSet>({
		create: () => Decoration.none,
		update(decorations, tr) {
			let next = decorations.map(tr.changes);
			for (const effect of tr.effects) {
				if (!effect.is(setErrorLine)) continue;
				if (effect.value == null) return Decoration.none;
				const number = Math.min(Math.max(effect.value, 1), tr.state.doc.lines);
				const line = tr.state.doc.line(number);
				next = Decoration.set([Decoration.line({ class: 'cm-error-line' }).range(line.from)]);
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
					errorLineField,
					Prec.high(
						keymap.of([
							{
								key: 'Shift-Mod-Enter',
								preventDefault: true,
								run: (v) => {
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
						if (u.docChanged) content = u.state.doc.toString();
					})
				]
			})
		});
		return () => {
			view?.destroy();
			view = undefined;
		};
	});

	$effect(() => {
		if (!view) return;
		view.dispatch({ effects: setErrorLine.of(errorLine ?? null) });
	});

	$effect(() => {
		if (!view) return;
		const current = view.state.doc.toString();
		if (content !== current) {
			view.dispatch({ changes: { from: 0, to: current.length, insert: content } });
		}
	});
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
		color: var(--sw-text);
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
</style>
