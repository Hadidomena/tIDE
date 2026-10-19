<script lang="ts">
	import { EditorState, Prec } from '@codemirror/state';
	import { EditorView, keymap } from '@codemirror/view';
	import { LanguageSupport } from '@codemirror/language';
	import { acceptCompletion, autocompletion, completeAnyWord } from '@codemirror/autocomplete';
	import { basicSetup } from 'codemirror';
	import { untrack } from 'svelte';
	import { tidalCompletion } from './vocabulary';
	import { tidalLanguage } from './tidal-mode';

	interface Props {
		content?: string;
		onEval?: (code: string) => void;
	}

	let { content = $bindable(''), onEval = () => {} }: Props = $props();

	let host: HTMLDivElement | undefined = $state();
	let view: EditorView | undefined;

	const tidalSupport = new LanguageSupport(tidalLanguage, [
		autocompletion({ override: [tidalCompletion, completeAnyWord] }),
		Prec.high(keymap.of([{ key: 'Tab', run: acceptCompletion }]))
	]);

	function lineOrSelection(v: EditorView): string {
		const { from, to } = v.state.selection.main;
		if (from !== to) return v.state.sliceDoc(from, to);
		return v.state.doc.lineAt(from).text;
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
					keymap.of([
						{
							key: 'Mod-Enter',
							preventDefault: true,
							run: (v) => {
								const code = lineOrSelection(v).trim();
								if (code) onEval(code);
								return true;
							}
						}
					]),
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
</style>
