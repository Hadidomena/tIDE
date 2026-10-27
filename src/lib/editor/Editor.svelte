<script lang="ts">
	import { EditorState, Prec } from '@codemirror/state';
	import { EditorView, keymap } from '@codemirror/view';
	import { LanguageSupport } from '@codemirror/language';
	import { acceptCompletion, autocompletion, completeAnyWord } from '@codemirror/autocomplete';
	import { indentLess, indentMore } from '@codemirror/commands';
	import { basicSetup } from 'codemirror';
	import { untrack } from 'svelte';
	import { tidalCompletion } from './vocabulary';
	import { tidalLanguage } from './tidal-mode';

	interface Props {
		content?: string;
		onEval?: (code: string) => void;
		onRunAll?: (code: string) => void;
		onHush?: () => void;
	}

	let {
		content = $bindable(''),
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

	function lineOrSelection(v: EditorView): string {
		const { from, to } = v.state.selection.main;
		if (from !== to) return v.state.sliceDoc(from, to);
		return v.state.doc.lineAt(from).text;
	}

	function evalLineOrSelection(v: EditorView): boolean {
		const code = lineOrSelection(v).trim();
		if (code) onEval(code);
		return true;
	}

	$effect(() => {
		if (!host) return;
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: untrack(() => content),
				extensions: [
					basicSetup,
					tidalSupport,
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
	}

	:global(.editor-host .cm-scroller) {
		font-family: 'JetBrains Mono', Menlo, Monaco, Consolas, monospace;
	}

	:global(.cm-tooltip.cm-tooltip-autocomplete) {
		background: #ffffff;
		color: #1e1e24;
		border: 1px solid #9a9aa2;
	}

	:global(.cm-tooltip-autocomplete > ul > li[aria-selected]) {
		background: #dbe6ff;
		color: #1e1e24;
	}

	:global(.cm-tooltip.cm-completionInfo) {
		background: #ffffff;
		color: #1e1e24;
		border: 1px solid #9a9aa2;
	}

	:global(.cm-completionDetail) {
		color: #4a4a52;
		font-style: italic;
	}

	@media (prefers-color-scheme: dark) {
		:global(.cm-tooltip.cm-tooltip-autocomplete) {
			background: #2a2a32;
			color: #f1f1f4;
			border-color: #55555f;
		}
		:global(.cm-tooltip.cm-completionInfo) {
			background: #2a2a32;
			color: #f1f1f4;
			border-color: #55555f;
		}
		:global(.cm-tooltip-autocomplete > ul > li[aria-selected]) {
			background: #3a4a6a;
			color: #ffffff;
		}
		:global(.cm-completionDetail) {
			color: #c8c8d0;
		}
	}
</style>
