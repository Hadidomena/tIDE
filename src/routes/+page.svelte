<script lang="ts">
	import Editor from '$lib/editor/Editor.svelte';
	import { session } from '$lib/session.svelte';

	let buffer = $state(`-- tIDE — live GHCi session (M1 step 3)
-- Cmd/Ctrl+Enter evaluates the current line or selection.
-- Try it: 1+1 · then let zz = 5*5 · then zz + 1  (stateful!)

slow 2 $ sound "bd sn:1 ~ hh"
-- the d1/slow eval prints "not in scope" until M2 boots a synth context
`);

	let consoleEl: HTMLPreElement | undefined = $state();
	let stickToBottom = true;

	$effect(() => {
		void session.connect();
	});

	$effect(() => {
		const last = session.chunks.at(-1);
		if (last && consoleEl && stickToBottom) {
			consoleEl.scrollTop = consoleEl.scrollHeight;
		}
	});

	function handleEval(code: string) {
		void session.eval(code);
	}

	function onScroll() {
		if (!consoleEl) return;
		stickToBottom = consoleEl.scrollHeight - consoleEl.scrollTop - consoleEl.clientHeight < 24;
	}
</script>

<header>
	<h1>tIDE</h1>
	<span class="chip {session.state}">
		<span class="dot"></span>
		session: {session.state}
	</span>
	{#if session.state === 'failed' || session.state === 'stopped'}
		<button class="restart" type="button" onclick={() => void session.restart()}>restart</button>
	{/if}
</header>

<main>
	<section class="editor-pane" aria-label="code editor">
		<Editor bind:content={buffer} onEval={handleEval} />
	</section>
	<section class="console-pane" aria-label="console" role="log">
		<pre bind:this={consoleEl} onscroll={onScroll}>{#each session.chunks as c (c.id)}<span
					class={c.stream}>{c.text}</span
				>{/each}</pre>
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
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.72rem;
		padding: 0.18rem 0.6rem;
		border-radius: 999px;
		background: color-mix(in srgb, currentColor 10%, transparent);
	}

	.dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 50%;
		background: #888;
	}

	.chip.ready .dot {
		background: #2ecc71;
	}
	.chip.busy .dot,
	.chip.starting .dot {
		background: #f1c40f;
		animation: pulse 0.8s infinite;
	}
	.chip.failed .dot {
		background: #e74c3c;
	}

	@keyframes pulse {
		50% {
			opacity: 0.25;
		}
	}

	.restart {
		font-size: 0.72rem;
		padding: 0.15rem 0.7rem;
		border-radius: 6px;
		border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
		background: transparent;
		color: inherit;
		cursor: pointer;
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
		overflow: hidden;
	}

	.console-pane pre {
		margin: 0;
		font-family: Menlo, Monaco, Consolas, monospace;
		font-size: 0.78rem;
		line-height: 1.45;
		white-space: pre-wrap;
		height: 100%;
		overflow-y: auto;
	}

	.stderr {
		color: #e5484d;
	}

	.system {
		color: #e8a33d;
		font-style: italic;
	}

	@media (prefers-color-scheme: dark) {
		:global(body) {
			background-color: #1e1e24;
			color: #eaeaea;
		}
		.stderr {
			color: #ff6369;
		}
	}
</style>
