import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import Database from '@tauri-apps/plugin-sql';
import { session } from './session.svelte';

export interface OpenedFile {
	path: string;
	content: string;
}

const TIDAL_FILTERS = [{ name: 'Tidal', extensions: ['tidal', 'hs'] }];

export const project = $state({
	path: '',
	title: 'untitled',
	hasLast: false
});

let db: Database | undefined;

async function getDb(): Promise<Database> {
	db ??= await Database.load('sqlite:tide.db');
	return db;
}

function splitPath(path: string) {
	const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
	return { dir: path.slice(0, i), file: path.slice(i + 1) };
}

async function registerProject(path: string) {
	try {
		const d = await getDb();
		const now = new Date().toISOString();
		const { dir, file } = splitPath(path);
		const rows = await d.select<{ id: number }[]>(
			`INSERT INTO projects (title, dir_path, entry_file, created_at, updated_at)
			 VALUES ($1, $2, $3, $4, $4)
			 ON CONFLICT(dir_path) DO UPDATE SET title = $1, entry_file = $3, updated_at = $4
			 RETURNING id`,
			[file, dir, file, now]
		);
		const id = rows[0]?.id;
		if (id !== undefined) {
			await d.execute(
				`INSERT INTO settings (key, value, updated_at)
				 VALUES ('last_project_id', $1, $2)
				 ON CONFLICT(key) DO UPDATE SET value = $1, updated_at = $2`,
				[String(id), now]
			);
			project.hasLast = true;
		}
	} catch (e) {
		session.notify(`project registration failed (file itself is saved): ${String(e)}`);
	}
}

export async function openProject(): Promise<OpenedFile | null> {
	const picked = await open({ multiple: false, filters: TIDAL_FILTERS });
	if (typeof picked !== 'string') return null;
	const file = await invoke<OpenedFile>('project_open', { path: picked });
	project.path = file.path;
	project.title = splitPath(file.path).file;
	await registerProject(file.path);
	return file;
}

export async function saveProject(content: string, saveAs = false): Promise<string | null> {
	let path = saveAs ? '' : project.path;
	if (!path) {
		const picked = await save({ defaultPath: 'main.tidal', filters: TIDAL_FILTERS });
		if (typeof picked !== 'string') return null;
		path = picked;
	}
	await invoke('project_save', { path, content });
	project.path = path;
	project.title = splitPath(path).file;
	await registerProject(path);
	return path;
}

export async function openLastProject(): Promise<OpenedFile | null> {
	try {
		const d = await getDb();
		const rows = await d.select<{ dir_path: string; entry_file: string }[]>(
			`SELECT p.dir_path, p.entry_file
			 FROM projects p
			 JOIN settings s ON s.key = 'last_project_id' AND s.value = CAST(p.id AS TEXT)
			 LIMIT 1`
		);
		const hit = rows[0];
		if (!hit) return null;
		const file = await invoke<OpenedFile>('project_open', {
			path: `${hit.dir_path}/${hit.entry_file}`
		});
		project.path = file.path;
		project.title = hit.entry_file;
		return file;
	} catch (e) {
		session.notify(`reopen last failed: ${String(e)}`);
		return null;
	}
}

export async function checkLastProject() {
	try {
		const d = await getDb();
		const rows = await d.select<{ n: number }[]>(
			`SELECT COUNT(*) AS n FROM settings WHERE key = 'last_project_id'`
		);
		project.hasLast = (rows[0]?.n ?? 0) > 0;
	} catch {
		// Comment to avoid empty block
	}
}
