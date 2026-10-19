import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type SessionState = 'stopped' | 'starting' | 'ready' | 'busy' | 'failed';

export interface Chunk {
	id: number;
	stream: 'stdout' | 'stderr' | 'system';
	text: string;
}

type WireEvent =
	| { kind: 'state'; state: SessionState }
	| { kind: 'output'; stream: 'stdout' | 'stderr'; text: string };

const MAX_CHUNKS = 2000;

class SessionModel {
	state = $state<SessionState>('stopped');
	chunks = $state<Chunk[]>([]);
	private connected = false;
	private unlisten: UnlistenFn | undefined;
	private seq = 0;

	async connect() {
		if (this.connected) return;
		if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
			this.push('system', 'not a Tauri window — use the app window from `npm run tauri dev`\n');
			return;
		}
		this.connected = true;
		this.unlisten = await listen<WireEvent>('session://event', (e) => this.handle(e.payload));
		await this.start();
	}

	private handle(ev: WireEvent) {
		if (ev.kind === 'state') {
			this.state = ev.state;
			return;
		}
		this.push(ev.stream, ev.text);
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

	private push(stream: Chunk['stream'], text: string) {
		this.chunks.push({ id: this.seq++, stream, text });
		if (this.chunks.length > MAX_CHUNKS) {
			this.chunks.splice(0, this.chunks.length - MAX_CHUNKS);
		}
	}

	async start() {
		try {
			this.state = await invoke<SessionState>('session_start');
		} catch (e) {
			this.push('system', `start failed: ${String(e)}\n`);
		}
	}

	async eval(code: string) {
		try {
			await invoke('session_eval', { code });
		} catch (e) {
			this.push('system', `eval rejected: ${String(e)}\n`);
		}
	}

	/** visible echo for console-typed input (ghci on pipes does not echo stdin itself) */
	echo(code: string) {
		this.push('system', `> ${code}\n`);
	}

	async restart() {
		try {
			this.state = await invoke<SessionState>('session_restart');
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
