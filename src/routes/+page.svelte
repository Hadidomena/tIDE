<script lang="ts">
	import Editor from '$lib/editor/Editor.svelte';

	let buffer = $state(`-- tIDE (M1 step 1) — editor shell with Tidal highlighting
-- Cmd/Ctrl+Enter requests eval of the current line/selection;
-- the live GHCi session replaces the echo console in step 3.

let beats = [1, 2, 3, 4]

d1 $ s "bd:1 sn*2 hh"
   # n (slow 2 $ segment 4 $ rand)
`);

	let evalEcho = $state('');

	// placeholder sink until session_eval lands (step 3)
	function handleEval(code: string) {
		evalEcho = `eval requested (no session yet):\n\n${code}`;
	}
</script>

<header>
	<h1>tIDE</h1>
	<span class="chip">session: idle — live in M1 step 3</span>
</header>

<main>
	<section class="editor-pane" aria-label="code editor">
		<Editor bind:content={buffer} onEval={handleEval} />
	</section>
	<section class="console-pane" aria-label="console">
		{#if evalEcho}
			<pre>{evalEcho}</pre>
		{:else}
			<p class="hint">console pane — the live GHCi stream lands with the session (step 3)</p>
		{/if}
	</section>
</main>

<style>
	:global(body) {
		margin: 0;
		font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
	}

	header {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.45rem 1rem;
		border-bottom: 1px solid color-mix(in srgb, currentColor 20%, transparent);
	}

	h1 {
		font-size: 1.05rem;
		margin: 0;
	}

	.chip {
		font-size: 0.72rem;
		padding: 0.18rem 0.6rem;
		border-radius: 999px;
		background: color-mix(in srgb, currentColor 10%, transparent);
	}

	main {
		display: grid;
		grid-template-columns: 3fr 2fr;
		height: calc(100vh - 2.7rem);
	}

	.editor-pane,
	.console-pane {
		min-width: 0;
		overflow: hidden;
	}

	.console-pane {
		border-left: 1px solid color-mix(in srgb, currentColor 20%, transparent);
		padding: 0.5rem 0.75rem;
		overflow-y: auto;
	}

	.console-pane pre,
	.hint {
		font-family: Menlo, Monaco, Consolas, monospace;
		font-size: 0.78rem;
		white-space: pre-wrap;
	}

	.hint {
		color: color-mix(in srgb, currentColor 55%, transparent);
	}

	@media (prefers-color-scheme: dark) {
		:global(body) {
			background-color: #1e1e24;
			color: #eaeaea;
		}
	}
</style>
