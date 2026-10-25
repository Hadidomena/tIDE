<script lang="ts">
	import Editor from '$lib/editor/Editor.svelte';
	import {
		checkLastProject,
		openLastProject,
		openProject,
		project,
		saveProject
	} from '$lib/project.svelte';
	import { session } from '$lib/session.svelte';

	let buffer = $state(`-- tIDE — live GHCi session with SuperDirt (M2)
-- Cmd/Ctrl+Enter evaluates the current line or selection.
-- Cmd/Ctrl+. (or the hush button) silences every running pattern.

d1 $ s "bd sn:1 ~ hh"
`);

	let consoleEl: HTMLPreElement | undefined = $state();
	let stickToBottom = true;
	let lineIn = $state('');
	let lastSaved = $state('');
	const dirty = $derived(buffer !== lastSaved);

	$effect(() => {
		void session.connect();
	});

	$effect(() => {
		void checkLastProject();
	});

	$effect(() => {
		const onKey = (e: KeyboardEvent) => {
			if (!(e.metaKey || e.ctrlKey)) return;
			const k = e.key.toLowerCase();
			if (k === 's') {
				e.preventDefault();
				void doSave(e.shiftKey);
			} else if (k === 'o') {
				e.preventDefault();
				void doOpen();
			}
		};
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	});

	async function doOpen() {
		try {
			const f = await openProject();
			if (f) {
				buffer = f.content;
				lastSaved = f.content;
				session.notify(`opened ${f.path}`);
			}
		} catch (e) {
			session.notify(`open failed: ${String(e)}`);
		}
	}

	async function doOpenLast() {
		const f = await openLastProject();
		if (f) {
			buffer = f.content;
			lastSaved = f.content;
			session.notify(`reopened ${f.path}`);
		}
	}

	async function doSave(saveAs = false) {
		try {
			const p = await saveProject(buffer, saveAs);
			if (p) {
				lastSaved = buffer;
				session.notify(`saved ${p}`);
			}
		} catch (e) {
			session.notify(`save failed: ${String(e)}`);
		}
	}

	$effect(() => {
		const last = session.chunks.at(-1);
		if (last && consoleEl && stickToBottom) {
			consoleEl.scrollTop = consoleEl.scrollHeight;
		}
	});

	function handleEval(code: string) {
		void session.eval(code);
	}

	function handleHush() {
		void session.hush();
	}

	function onConsoleKey(e: KeyboardEvent) {
		if (e.key !== 'Enter') return;
		const code = lineIn.trim();
		if (!code) return;
		session.echo(code);
		void session.eval(code);
		lineIn = '';
	}

	function onScroll() {
		if (!consoleEl) return;
		stickToBottom = consoleEl.scrollHeight - consoleEl.scrollTop - consoleEl.clientHeight < 24;
	}
</script>

<header>
	<h1>tIDE</h1>
	<span class="file" title={project.path || undefined}>
		{project.title}{#if dirty}<span class="star">*</span>{/if}
	</span>
	<div class="tools">
		<button class="tbtn" type="button" onclick={doOpen}>open</button>
		<button class="tbtn" type="button" onclick={() => void doSave(false)}>save</button>
		{#if project.hasLast}
			<button class="tbtn" type="button" onclick={doOpenLast}>last</button>
		{/if}
		<button
			class="tbtn"
			type="button"
			title="silence all patterns (Cmd/Ctrl+.)"
			disabled={session.state !== 'ready'}
			onclick={handleHush}>hush</button
		>
	</div>
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
		<Editor bind:content={buffer} onEval={handleEval} onHush={handleHush} />
	</section>
	<section class="console-pane" aria-label="console">
		<pre
			bind:this={consoleEl}
			onscroll={onScroll}
			role="log">{#each session.chunks as c (c.id)}<span class={c.stream}>{c.text}</span
				>{/each}</pre>
		<div class="console-row">
			<span class="prompt">›</span>
			<input
				bind:value={lineIn}
				onkeydown={onConsoleKey}
				placeholder="type Haskell or :commands, Enter evaluates"
				aria-label="console input"
			/>
		</div>
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

	.restart,
	.tbtn {
		font-size: 0.72rem;
		padding: 0.15rem 0.7rem;
		border-radius: 6px;
		border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
		background: transparent;
		color: inherit;
		cursor: pointer;
	}

	.tbtn:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.tools {
		display: inline-flex;
		gap: 0.35rem;
	}

	.file {
		font-size: 0.78rem;
		opacity: 0.85;
		max-width: 22rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.star {
		font-weight: 700;
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
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		border-left: 1px solid color-mix(in srgb, currentColor 20%, transparent);
		padding: 0.5rem 0.75rem;
		overflow: hidden;
	}

	.console-pane pre {
		margin: 0;
		flex: 1;
		min-height: 0;
		font-family: Menlo, Monaco, Consolas, monospace;
		font-size: 0.78rem;
		line-height: 1.45;
		white-space: pre-wrap;
		overflow-y: auto;
	}

	.console-row {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding-top: 0.3rem;
		border-top: 1px solid color-mix(in srgb, currentColor 15%, transparent);
	}

	.console-row .prompt {
		font-family: Menlo, monospace;
		color: #2ecc71;
	}

	.console-row input {
		flex: 1;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		color: inherit;
		font-family: Menlo, Monaco, Consolas, monospace;
		font-size: 0.78rem;
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
