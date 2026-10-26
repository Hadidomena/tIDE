import Database from '@tauri-apps/plugin-sql';

let db: Database | undefined;

export async function getDb(): Promise<Database> {
	db ??= await Database.load('sqlite:tide.db');
	return db;
}

export async function getSetting(key: string): Promise<string | null> {
	const rows = await (
		await getDb()
	).select<{ value: string }[]>('SELECT value FROM settings WHERE key = $1', [key]);
	return rows[0]?.value ?? null;
}

export async function setSetting(key: string, value: string): Promise<void> {
	await (
		await getDb()
	).execute(
		`INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, $3)
		 ON CONFLICT(key) DO UPDATE SET value = $2, updated_at = $3`,
		[key, value, new Date().toISOString()]
	);
}
