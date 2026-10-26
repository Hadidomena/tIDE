<script lang="ts">
	import { onMount } from 'svelte';
	import Editor from '$lib/editor/Editor.svelte';
	import {
		checkLastProject,
		openLastProject,
		openProject,
		project,
		saveProject
	} from '$lib/project.svelte';
	import {
		detectToolchain,
		diagnostics,
		initDiagnostics,
		resolveEngine,
		setEngineChoice,
		type EngineChoice
	} from '$lib/diagnostics.svelte';
	import { session } from '$lib/session.svelte';

	const ENGINE_CHOICES: EngineChoice[] = ['auto', 'superdirt', 'headless'];

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

	onMount(() => {
		void (async () => {
			await initDiagnostics();
			await session.connect(resolveEngine());
			void detectToolchain();
		})();
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

	async function copyHint(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			session.notify('hint copied to clipboard');
		} catch {
			session.notify(text);
		}
	}

	function restartWithEngine() {
		void session.restart(resolveEngine());
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
		<button
			class="tbtn"
			type="button"
			onclick={() => (diagnostics.open = !diagnostics.open)}
			aria-pressed={diagnostics.open}>{diagnostics.open ? 'hide diag' : 'diagnostics'}</button
		>
	</div>
	<span class="chip {session.state}">
		<span class="dot"></span>
		session: {session.state}
	</span>
	{#if session.engine}
		<span class="chip {session.engine.status}" title={session.engine.detail}>
			<span class="dot"></span>
			engine: {session.engine.engine} · {session.engine.status}
		</span>
	{/if}
	{#if session.state === 'failed' || session.state === 'stopped'}
		<button class="restart" type="button" onclick={restartWithEngine}>restart</button>
	{/if}
</header>

<main class:with-diag={diagnostics.open}>
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
	{#if diagnostics.open}
		<aside class="diag" aria-label="diagnostics">
			<div class="diag-head">
				<strong>diagnostics</strong>
				<button
					class="tbtn"
					type="button"
					disabled={diagnostics.detecting}
					onclick={() => void detectToolchain()}
					>{diagnostics.detecting ? 'detecting…' : 're-detect'}</button
				>
			</div>

			<div class="section">
				<span class="label">engine</span>
				<div class="seg">
					{#each ENGINE_CHOICES as choice (choice)}
						<button
							class="segbtn"
							class:on={diagnostics.choice === choice}
							type="button"
							onclick={() => void setEngineChoice(choice)}>{choice}</button
						>
					{/each}
				</div>
				{#if session.engine && session.engine.engine !== resolveEngine()}
					<span class="muted">change applies on restart</span>
				{/if}
				<button class="tbtn" type="button" onclick={restartWithEngine}>restart session</button>
			</div>

			{#if diagnostics.error}
				<p class="error">{diagnostics.error}</p>
			{/if}

			{#if diagnostics.report}
				<div class="status-line">
					toolchain: <b>{diagnostics.report.status}</b>
				</div>
				<div class="probes">
					{#each diagnostics.report.probes as p (p.name)}
						<div class="probe {p.status}">
							<span class="dot"></span>
							<div class="probe-text">
								<div><b>{p.name}</b></div>
								<div class="muted">{p.detail}</div>
								{#if p.hint}
									<button class="hint" type="button" onclick={() => void copyHint(p.hint)}>
										{p.hint}
									</button>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			{:else}
				<p class="muted">no detection yet — press re-detect</p>
			{/if}
		</aside>
	{/if}
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

	main.with-diag {
		grid-template-columns: 3fr 2fr 22rem;
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

	.engine {
		color: #6f8fd6;
	}

	.system {
		color: #e8a33d;
		font-style: italic;
	}

	.diag {
		border-left: 1px solid color-mix(in srgb, currentColor 20%, transparent);
		padding: 0.55rem 0.75rem;
		overflow-y: auto;
		font-size: 0.74rem;
		line-height: 1.4;
	}

	.diag-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		margin-bottom: 0.6rem;
	}

	.diag .section {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.4rem;
		margin-bottom: 0.7rem;
	}

	.label {
		font-size: 0.6rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		opacity: 0.6;
	}

	.seg {
		display: inline-flex;
		border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
		border-radius: 6px;
		overflow: hidden;
	}

	.segbtn {
		font: inherit;
		font-size: 0.7rem;
		padding: 0.18rem 0.55rem;
		border: none;
		background: transparent;
		color: inherit;
		cursor: pointer;
	}

	.segbtn + .segbtn {
		border-left: 1px solid color-mix(in srgb, currentColor 20%, transparent);
	}

	.segbtn.on {
		background: color-mix(in srgb, currentColor 18%, transparent);
	}

	.status-line {
		margin: 0 0 0.55rem;
	}

	.probes {
		display: flex;
		flex-direction: column;
		gap: 0.55rem;
	}

	.probe {
		display: flex;
		align-items: flex-start;
		gap: 0.45rem;
	}

	.probe .dot {
		margin-top: 0.3rem;
		flex: none;
	}

	.probe.ok .dot {
		background: #2ecc71;
	}

	.probe.warn .dot {
		background: #f1c40f;
	}

	.probe.missing .dot {
		background: #e74c3c;
	}

	.probe-text {
		min-width: 0;
		word-break: break-word;
	}

	.muted {
		opacity: 0.65;
	}

	.hint {
		display: block;
		margin-top: 0.15rem;
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		font: inherit;
		font-size: 0.68rem;
		text-align: left;
		opacity: 0.8;
		text-decoration: underline dotted;
		cursor: pointer;
	}

	.error {
		color: #e5484d;
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
