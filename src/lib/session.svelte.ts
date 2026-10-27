import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type SessionState = 'stopped' | 'starting' | 'ready' | 'busy' | 'failed';

export interface Chunk {
	id: number;
	stream: 'stdout' | 'stderr' | 'system' | 'engine';
	text: string;
}

export interface EngineStatus {
	engine: 'superdirt' | 'headless';
	status: 'starting' | 'live' | 'down';
	detail: string;
}

type WireEvent =
	| { kind: 'state'; state: SessionState }
	| { kind: 'output'; stream: 'stdout' | 'stderr' | 'engine'; text: string }
	| ({ kind: 'engine' } & EngineStatus);

const MAX_CHUNKS = 2000;

class SessionModel {
	state = $state<SessionState>('stopped');
	engine = $state<EngineStatus | null>(null);
	errorLine = $state<number | null>(null);
	chunks = $state<Chunk[]>([]);
	private connected = false;
	private unlisten: UnlistenFn | undefined;
	private seq = 0;
	private outputTail = '';

	async connect(
		engine: string | null = null,
		audioIn: string | null = null,
		audioOut: string | null = null
	) {
		if (this.connected) return;
		if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
			this.push('system', 'not a Tauri window — use the app window from `npm run tauri dev`\n');
			return;
		}
		this.connected = true;
		this.unlisten = await listen<WireEvent>('session://event', (e) => this.handle(e.payload));
		await this.start(engine, audioIn, audioOut);
	}

	private handle(ev: WireEvent) {
		if (ev.kind === 'state') {
			this.state = ev.state;
			return;
		}
		if (ev.kind === 'engine') {
			this.engine = { engine: ev.engine, status: ev.status, detail: ev.detail };
			return;
		}
		this.push(ev.stream, ev.text);
		this.detectErrorLine(ev.text);
		if (
			ev.stream === 'stderr' &&
			/failed to spawn ghci|cannot satisfy -package tidal/.test(ev.text)
		) {
			this.push(
				'system',
				'hint: Tidal missing — see README prerequisites (cabal install --lib tidal)\n'
			);
		}
	}

	private detectErrorLine(text: string) {
		const combined = this.outputTail + text;
		this.outputTail = combined.slice(-256);
		const match = /tIDE\.hs:(\d+):\d+/.exec(combined);
		if (match) this.errorLine = Number(match[1]);
	}

	private push(stream: Chunk['stream'], text: string) {
		this.chunks.push({ id: this.seq++, stream, text });
		if (this.chunks.length > MAX_CHUNKS) {
			this.chunks.splice(0, this.chunks.length - MAX_CHUNKS);
		}
	}

	async start(
		engine: string | null = null,
		audioIn: string | null = null,
		audioOut: string | null = null
	) {
		try {
			this.state = await invoke<SessionState>('session_start', { engine, audioIn, audioOut });
		} catch (e) {
			this.push('system', `start failed: ${String(e)}\n`);
		}
	}

	async hush() {
		this.echo('hush');
		await this.eval('hush');
	}

	async eval(code: string, line?: number) {
		this.errorLine = null;
		try {
			await invoke('session_eval', { code, line: line ?? null });
		} catch (e) {
			this.push('system', `eval rejected: ${String(e)}\n`);
		}
	}

	async runFile(code: string) {
		this.errorLine = null;
		try {
			const count = await invoke<number>('session_eval_file', { code });
			this.notify(`running ${count} statement${count === 1 ? '' : 's'} from the file`);
		} catch (e) {
			this.push('system', `run file failed: ${String(e)}\n`);
		}
	}

	echo(code: string) {
		this.push('system', `> ${code}\n`);
	}
	notify(text: string) {
		this.push('system', text.endsWith('\n') ? text : `${text}\n`);
	}

	async restart(
		engine: string | null = null,
		audioIn: string | null = null,
		audioOut: string | null = null
	) {
		try {
			this.state = await invoke<SessionState>('session_restart', { engine, audioIn, audioOut });
		} catch (e) {
			this.push('system', `restart failed: ${String(e)}\n`);
		}
	}

	destroy() {
		this.unlisten?.();
		this.unlisten = undefined;
		this.connected = false;
	}
}

export const session = new SessionModel();
