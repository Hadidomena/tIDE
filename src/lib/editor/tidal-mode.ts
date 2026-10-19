import { StreamLanguage, StringStream } from '@codemirror/language';

interface TidalState {
	/** close token of the active {- -} / {-- --} block comment; '' when none */
	blockCloser: string;
}

const haskellKeywords = new Set([
	'let',
	'where',
	'import',
	'module',
	'case',
	'of',
	'do',
	'if',
	'then',
	'else',
	'type',
	'data',
	'newtype',
	'class',
	'instance',
	'infix',
	'infixl',
	'infixr',
	'qualified',
	'as',
	'hiding',
	'deriving',
	'forall'
]);

/** TidalCycles DSL vocabulary — keyword-colored for scannability (plan-v2 §4.1). */
const tidalOps = new Set([
	'stack',
	'sequence',
	'polyrhythm',
	'poly',
	'every',
	'slow',
	'fast',
	'jux',
	'off',
	'chunkBy',
	'ply',
	'rev',
	'break',
	'degrade',
	'degradeBy',
	'unDegradeBy',
	'armistice',
	'perlin',
	'rand',
	'scramble',
	'superimpose',
	'enrich',
	'stretch',
	'compress',
	'squeeze',
	'iter',
	'timeLoop',
	'sometimes',
	'often',
	'almostAlways',
	'never',
	'sometimesBy',
	'rotL',
	'rotR',
	'fake',
	'hurry',
	'segment',
	'quantize',
	'discrete',
	'clear',
	'setms'
]);

/** ordered longest-first so '::>' wins over '::' over ':' */
const operators = [
	'::>',
	'|>',
	'<>',
	'>>',
	'<<',
	'=>',
	'~>',
	'~',
	'$',
	'#',
	'%',
	'?',
	'&',
	'|',
	'+',
	'-',
	'*',
	'/',
	'^',
	'::',
	':',
	'.',
	'=',
	'<',
	'>',
	'!',
	'\\',
	'`',
	';'
];

function consumeString(stream: StringStream): void {
	// consume to closing quote or EOL — Haskell string literals do not span lines
	while (!stream.eol()) {
		const ch = stream.next();
		if (ch === '\\') {
			stream.next();
			continue;
		}
		if (ch === '"') return;
	}
}

function scanBlockComment(stream: StringStream, state: TidalState): void {
	const closer = state.blockCloser;
	const opener = closer === '--}' ? '{--' : '{-';
	while (!stream.eol()) {
		if (stream.match(closer)) {
			state.blockCloser = '';
			return;
		}
		if (stream.match(opener)) continue; // nested open: first closer ends it (coloring-only simplification)
		stream.next();
	}
}

export const tidalLanguage = StreamLanguage.define<TidalState>({
	name: 'tidal',

	startState(): TidalState {
		return { blockCloser: '' };
	},

	token(stream: StringStream, state: TidalState): string | null {
		// inside a block comment spanning lines
		if (state.blockCloser) {
			scanBlockComment(stream, state);
			return 'comment';
		}

		if (stream.eatSpace()) return null;

		if (stream.match('--')) {
			stream.skipToEnd();
			return 'comment';
		}

		if (stream.match('{--')) {
			state.blockCloser = '--}';
			scanBlockComment(stream, state);
			return 'comment';
		}
		if (stream.match('{-')) {
			state.blockCloser = '-}';
			scanBlockComment(stream, state);
			return 'comment';
		}

		if (stream.match(/^\d+(\.\d+)?([eE][+-]?\d+)?/)) return 'number';

		if (stream.eat('"')) {
			consumeString(stream);
			return 'string';
		}

		if (stream.match(/^[A-Za-z_][A-Za-z0-9_']*/)) {
			const word = stream.current();
			if (haskellKeywords.has(word)) return 'keyword';
			if (/^[dvn][1-9]$/.test(word) || tidalOps.has(word)) return 'operatorKeyword';
			if (/^[A-Z]/.test(word)) return 'typeName';
			return null;
		}

		for (const op of operators) {
			if (stream.match(op, true)) return 'operator';
		}

		if (stream.match(/[()[\]{}]/)) return 'bracket';

		stream.next();
		return null;
	},

	languageData: {
		commentTokens: { line: '--', block: { open: '{--', close: '--}' } },
		indentString: '  '
	}
});
