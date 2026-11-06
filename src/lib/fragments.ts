export interface TextRange {
	from: number;
	to: number;
}

export interface OrbitCode {
	line: TextRange;
	tokens: Map<string, TextRange[]>;
}

interface SourceLine {
	from: number;
	to: number;
	text: string;
}

const ORBIT_START = /^[ \t]*d(1[0-6]|[1-9])\b/;
const SOUND_LITERAL = /\b(?:s|sound)\s+"((?:[^"\\]|\\.)*)"/g;
const TOKEN_MODIFIER = /[:*!?@/]/;

export function indexOrbits(source: string): Map<number, OrbitCode> {
	const lines = splitLines(source);
	const index = new Map<number, OrbitCode>();
	let i = 0;
	while (i < lines.length) {
		const match = ORBIT_START.exec(lines[i].text);
		if (!match) {
			i += 1;
			continue;
		}
		const orbit = Number(match[1]) - 1;
		let end = i + 1;
		while (end < lines.length) {
			const next = lines[end].text;
			if (next.trim() !== '' && !/^[ \t]/.test(next)) break;
			end += 1;
		}

		const statement = { from: lines[i].from, to: lines[end - 1].to };
		const tokens = collectTokens(source, statement);
		index.set(orbit, { line: { from: lines[i].from, to: lines[i].to }, tokens });
		i = end;
	}
	return index;
}

export function rangesFor(
	index: Map<number, OrbitCode>,
	orbit: number,
	sample: string
): TextRange[] {
	const entry = index.get(orbit);
	if (!entry) return [];
	const name = normalizeSample(sample);
	if (!name) return [entry.line];

	const direct = entry.tokens.get(name);
	if (direct?.length) return direct;

	const tail = name.slice(name.lastIndexOf('_') + 1);
	if (tail && tail !== name) {
		const byTail = entry.tokens.get(tail);
		if (byTail?.length) return byTail;
	}
	return [entry.line];
}

function tokenNames(token: string): string[] {
	const names = new Set<string>();
	const add = (name: string) => {
		if (name && /[a-z0-9]/i.test(name)) names.add(name);
	};
	add(normalizeToken(token));
	const colon = token.lastIndexOf(':');
	if (colon >= 0) add(normalizeToken(token.slice(colon + 1)));
	return [...names];
}

export function normalizeToken(token: string): string {
	const cut = token.search(TOKEN_MODIFIER);
	const base = cut >= 0 ? token.slice(0, cut) : token;
	return base.replace(/[[\](){}^_,]/g, '').trim();
}

export function normalizeSample(sample: string): string {
	const trimmed = sample.trim();
	if (!trimmed.includes(':')) return trimmed;
	return trimmed.slice(trimmed.lastIndexOf(':') + 1);
}

function collectTokens(source: string, statement: TextRange): Map<string, TextRange[]> {
	const tokens = new Map<string, TextRange[]>();
	const text = source.slice(statement.from, statement.to);
	SOUND_LITERAL.lastIndex = 0;
	let literal: RegExpExecArray | null;
	while ((literal = SOUND_LITERAL.exec(text))) {
		const content = literal[1];
		const contentStart = statement.from + literal.index + literal[0].indexOf('"') + 1;
		const tokenRe = /\S+/g;
		let token: RegExpExecArray | null;
		while ((token = tokenRe.exec(content))) {
			const range = {
				from: contentStart + token.index,
				to: contentStart + token.index + token[0].length
			};
			for (const name of tokenNames(token[0])) {
				const list = tokens.get(name) ?? [];
				list.push(range);
				tokens.set(name, list);
			}
		}
	}
	return tokens;
}

function splitLines(source: string): SourceLine[] {
	const lines: SourceLine[] = [];
	let from = 0;
	for (const text of source.split('\n')) {
		lines.push({ from, to: from + text.length, text });
		from += text.length + 1;
	}
	return lines;
}
