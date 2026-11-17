export interface TextRange {
	from: number;
	to: number;
}

export interface OrbitCode {
	orbit: number;
	startLine: number;
	line: TextRange;
	tokens: Map<string, TextRange[]>;
}

interface Binding {
	name: string;
	line: TextRange;
	tokens: Map<string, TextRange[]>;
}

interface SourceLine {
	from: number;
	to: number;
	text: string;
}

const ORBIT_START = /^[ \t]*d(1[0-6]|[1-9])\b/;
const LET_DEF = /^[ \t]*let\s+([a-z_][A-Za-z0-9_']*)\s*=/;
const IDENTIFIER = /\b[a-z_][A-Za-z0-9_']*\b/g;
const SOUND_LITERAL = /\b(?:s|sound)\s+"((?:[^"\\]|\\.)*)"/g;
const TOKEN_MODIFIER = /[:*!?@/^]/;

export function indexOrbits(source: string): OrbitIndex {
	const lines = splitLines(source);
	const bindings = indexBindings(lines, source);
	const index: OrbitIndex = { byOrbit: new Map() };
	let i = 0;
	while (i < lines.length) {
		const match = ORBIT_START.exec(lines[i].text);
		if (!match) {
			i += 1;
			continue;
		}
		const orbit = Number(match[1]) - 1;
		const end = statementEnd(lines, i);
		const statement = { from: lines[i].from, to: lines[end - 1].to };
		const own = collectTokens(source, statement);
		const used = referencedBindings(lines, source, i, end, bindings);
		const tokens = new Map(own);
		for (const binding of used) {
			for (const [name, ranges] of binding.tokens) {
				if (!tokens.has(name)) tokens.set(name, ranges);
			}
		}
		const line =
			own.size === 0 && used.length > 0 ? used[0].line : { from: lines[i].from, to: lines[i].to };
		const statements = index.byOrbit.get(orbit) ?? [];
		statements.push({ orbit, startLine: i + 1, line, tokens });
		index.byOrbit.set(orbit, statements);
		i = end;
	}
	return index;
}

function statementEnd(lines: SourceLine[], start: number): number {
	let end = start + 1;
	while (end < lines.length) {
		const line = lines[end];
		const trimmed = line.text.trim();
		if (trimmed === '' || /^[ \t]/.test(line.text)) {
			end += 1;
			continue;
		}
		if (trimmed.startsWith('--')) {
			let look = end + 1;
			while (look < lines.length && lines[look].text.trim() === '') look += 1;
			if (look < lines.length && /^[ \t]/.test(lines[look].text)) {
				end += 1;
				continue;
			}
		}
		break;
	}
	return end;
}

export interface OrbitIndex {
	byOrbit: Map<number, OrbitCode[]>;
}

function indexBindings(lines: SourceLine[], source: string): Map<string, Binding> {
	const bindings = new Map<string, Binding>();
	let i = 0;
	while (i < lines.length) {
		const match = LET_DEF.exec(lines[i].text);
		if (!match) {
			i += 1;
			continue;
		}
		const end = statementEnd(lines, i);
		const statement = { from: lines[i].from, to: lines[end - 1].to };
		bindings.set(match[1], {
			name: match[1],
			line: { from: lines[i].from, to: lines[i].to },
			tokens: collectTokens(source, statement)
		});
		i = end;
	}
	return bindings;
}

function referencedBindings(
	lines: SourceLine[],
	source: string,
	start: number,
	end: number,
	bindings: Map<string, Binding>
): Binding[] {
	if (bindings.size === 0) return [];
	const text = maskLineComments(source.slice(lines[start].from, lines[end - 1].to));
	const used: Binding[] = [];
	const seen = new Set<string>();
	for (const match of text.matchAll(IDENTIFIER)) {
		const binding = bindings.get(match[0]);
		if (binding && !seen.has(binding.name)) {
			seen.add(binding.name);
			used.push(binding);
		}
	}
	return used;
}

export function rangesFor(
	index: OrbitIndex,
	orbit: number,
	sample: string,
	line?: number
): TextRange[] {
	const statements = index.byOrbit.get(orbit);
	if (!statements || statements.length === 0) return [];
	const entry =
		(line === undefined ? undefined : statements.find((s) => s.startLine === line)) ??
		statements[statements.length - 1];
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
	for (const part of token.split(',')) {
		add(normalizeToken(part));
		const colon = part.lastIndexOf(':');
		if (colon >= 0) add(normalizeToken(part.slice(colon + 1)));
	}
	return [...names];
}

export function insideSoundLiteral(textBefore: string): boolean {
	const code = stripLineComment(textBefore);
	return /(?:^|[\s$(#])(?:s|sound)\s*"[^"]*$/.test(code);
}

function stripLineComment(text: string): string {
	let inString = false;
	for (let i = 0; i < text.length; i += 1) {
		const ch = text[i];
		if (ch === '"') {
			inString = !inString;
		} else if (!inString && ch === '-' && text[i + 1] === '-') {
			return text.slice(0, i);
		}
	}
	return text;
}

export function normalizeToken(token: string): string {
	const cut = token.search(TOKEN_MODIFIER);
	const base = cut >= 0 ? token.slice(0, cut) : token;
	return base.replace(/[[\](){}]/g, '').trim();
}

export function normalizeSample(sample: string): string {
	const trimmed = sample.trim();
	if (!trimmed.includes(':')) return trimmed;
	return trimmed.slice(trimmed.lastIndexOf(':') + 1);
}

function collectTokens(source: string, statement: TextRange): Map<string, TextRange[]> {
	const tokens = new Map<string, TextRange[]>();
	const text = maskLineComments(source.slice(statement.from, statement.to));
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

function maskLineComments(text: string): string {
	return text
		.split('\n')
		.map((line) => {
			const at = line.indexOf('--');
			return at >= 0 ? line.slice(0, at) + ' '.repeat(line.length - at) : line;
		})
		.join('\n');
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
