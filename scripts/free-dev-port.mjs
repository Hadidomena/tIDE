#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const PORT = 1420;
const ROOT = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
const isWindows = process.platform === 'win32';

const run = (cmd, args) => {
	try {
		return execFileSync(cmd, args, { encoding: 'utf8' }).trim();
	} catch {
		return '';
	}
};

function listeningPids() {
	if (isWindows) {
		const out = run('netstat', ['-ano', '-p', 'tcp'])
			.split('\n')
			.filter((line) => line.includes(`:${PORT}`) && line.toUpperCase().includes('LISTENING'))
			.map((line) => line.trim().split(/\s+/).at(-1))
			.filter(Boolean);
		return [...new Set(out)];
	}
	return [
		...new Set(run('lsof', ['-nP', `-iTCP:${PORT}`, '-sTCP:LISTEN', '-t']).split('\n'))
	].filter(Boolean);
}

function commandOf(pid) {
	if (isWindows) return '';
	return run('ps', ['-ww', '-p', pid, '-o', 'command=']);
}

function isOurVite(pid) {
	const command = commandOf(pid);
	return command.includes(ROOT) && command.includes('vite');
}

const pids = listeningPids();
if (pids.length === 0) process.exit(0);

const stale = pids.filter(isOurVite);
const foreign = pids.filter((pid) => !stale.includes(pid));

if (foreign.length > 0) {
	const detail = foreign
		.map((pid) => {
			const command = commandOf(pid);
			return command ? `pid ${pid}: ${command.slice(0, 120)}` : `pid ${pid}`;
		})
		.join('; ');
	console.error(`[dev] port ${PORT} is held by another process (${detail}); stop it and retry.`);
	process.exit(1);
}

for (const pid of stale) {
	console.log(`[dev] stopping stale vite on port ${PORT} (pid ${pid})`);
	try {
		process.kill(Number(pid), 'SIGTERM');
	} catch {
		// already gone
	}
}

const deadline = Date.now() + 2000;
while (Date.now() < deadline) {
	if (listeningPids().length === 0) process.exit(0);
	await new Promise((resolve) => setTimeout(resolve, 100));
}

for (const pid of listeningPids()) {
	try {
		process.kill(Number(pid), 'SIGKILL');
	} catch {
		// already gone
	}
}
