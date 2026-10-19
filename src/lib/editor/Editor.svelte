<script lang="ts">
	import { EditorState } from '@codemirror/state';
	import { EditorView, keymap } from '@codemirror/view';
	import { LanguageSupport } from '@codemirror/language';
	import { basicSetup } from 'codemirror';
	import { tidalLanguage } from './tidal-mode';

	interface Props {
		/** current buffer text, two-way (project open/save in step 5 uses this) */
		content?: string;
		/** called with the line or selection under the cursor on Mod-Enter (M1 step 3 wires the session) */
		onEval?: (code: string) => void;
	}

	let { content = $bindable(''), onEval = () => {} }: Props = $props();

	let host: HTMLDivElement | undefined = $state();
	let view: EditorView | undefined;

	const tidalSupport = new LanguageSupport(tidalLanguage);

	/** eval semantics per plan-v2 §4.1: selection if any, else the whole current line */
	function lineOrSelection(v: EditorView): string {
		const { from, to } = v.state.selection.main;
		if (from !== to) return v.state.sliceDoc(from, to);
		return v.state.doc.lineAt(from).text;
	}

	// mount once the host element exists
	$effect(() => {
		if (!host) return;
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: content,
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
					// typing -> prop (keeps parent's bind:content in sync)
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

	// external prop change (project open in step 5) -> editor; guarded against echo loops
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
