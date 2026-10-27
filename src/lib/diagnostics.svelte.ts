import { invoke } from '@tauri-apps/api/core';
import { getSetting, setSetting } from './db';

export type EngineChoice = 'auto' | 'superdirt' | 'headless';
export type ToolchainStatus = 'superDirtReady' | 'headlessOnly' | 'broken';

export interface ProbeResult {
	name: string;
	status: 'ok' | 'warn' | 'missing';
	detail: string;
	hint: string;
}

export interface DetectReport {
	status: ToolchainStatus;
	probes: ProbeResult[];
}

export const diagnostics = $state({
	open: false,
	choice: 'auto' as EngineChoice,
	report: null as DetectReport | null,
	cachedStatus: null as ToolchainStatus | null,
	detecting: false,
	error: ''
});

export async function initDiagnostics() {
	try {
		const choice = await getSetting('engine');
		if (choice === 'auto' || choice === 'superdirt' || choice === 'headless') {
			diagnostics.choice = choice;
		}
		const cached = await getSetting('toolchain_status');
		if (cached === 'superDirtReady' || cached === 'headlessOnly' || cached === 'broken') {
			diagnostics.cachedStatus = cached;
		}
	} catch (e) {
		diagnostics.error = String(e);
	}
}

export function resolveEngine(): 'superdirt' | 'headless' {
	if (diagnostics.choice === 'headless') return 'headless';
	if (diagnostics.choice === 'superdirt') return 'superdirt';
	const status = diagnostics.report?.status ?? diagnostics.cachedStatus;
	return status === 'headlessOnly' || status === 'broken' ? 'headless' : 'superdirt';
}

export async function setEngineChoice(choice: EngineChoice) {
	diagnostics.choice = choice;
	try {
		await setSetting('engine', choice);
	} catch (e) {
		diagnostics.error = String(e);
	}
}

async function overridesJson(): Promise<string | null> {
	const [ghc, sclang] = await Promise.all([
		getSetting('ghc_path_override').catch(() => null),
		getSetting('sclang_path_override').catch(() => null)
	]);
	if (!ghc && !sclang) return null;
	return JSON.stringify({ ghcPath: ghc ?? undefined, sclangPath: sclang ?? undefined });
}

export async function detectToolchain() {
	if (diagnostics.detecting) return;
	diagnostics.detecting = true;
	diagnostics.error = '';
	try {
		const overrides = await overridesJson();
		const report = await invoke<DetectReport>('toolchain_detect', { overrides });
		diagnostics.report = report;
		diagnostics.cachedStatus = report.status;
		await setSetting('toolchain_status', report.status).catch(() => {});
	} catch (e) {
		diagnostics.error = String(e);
	} finally {
		diagnostics.detecting = false;
	}
}
