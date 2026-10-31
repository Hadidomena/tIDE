use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repeat {
    pub hash: String,
    pub code: String,
    pub token_len: usize,
    pub occurrences: usize,
    pub spans: Vec<Span>,
}

pub fn tokenize(code: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;

    while i < code.len() {
        let c = char_at(code, i);

        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }

        if code[i..].starts_with("--") {
            i += 2;
            while i < code.len() && char_at(code, i) != '\n' {
                i += char_at(code, i).len_utf8();
            }
            continue;
        }

        if code[i..].starts_with("{-") {
            i += 2;
            let mut depth = 1usize;
            while i < code.len() && depth > 0 {
                if code[i..].starts_with("{-") {
                    depth += 1;
                    i += 2;
                } else if code[i..].starts_with("-}") {
                    depth -= 1;
                    i += 2;
                } else {
                    if char_at(code, i) == '\n' {
                        line += 1;
                    }
                    i += char_at(code, i).len_utf8();
                }
            }
            continue;
        }

        if c == '"' {
            let start = i;
            let start_line = line;
            i += 1;
            while i < code.len() {
                let ch = char_at(code, i);
                if ch == '\\' {
                    i += 1;
                    if i < code.len() {
                        let escaped = char_at(code, i);
                        if escaped == '\n' {
                            line += 1;
                        }
                        i += escaped.len_utf8();
                    }
                } else if ch == '"' {
                    i += 1;
                    break;
                } else {
                    if ch == '\n' {
                        line += 1;
                    }
                    i += ch.len_utf8();
                }
            }
            push_token(&mut tokens, code, start, i, start_line, line);
            continue;
        }

        if is_ident_start(c) {
            let start = i;
            i += c.len_utf8();
            while i < code.len() {
                let ch = char_at(code, i);
                if !is_ident_continue(ch) {
                    break;
                }
                i += ch.len_utf8();
            }
            if !is_statement_name(&code[start..i]) {
                push_token(&mut tokens, code, start, i, line, line);
            }
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < code.len() && char_at(code, i).is_ascii_digit() {
                i += 1;
            }
            if i + 1 < code.len()
                && code.as_bytes()[i] == b'.'
                && code.as_bytes()[i + 1].is_ascii_digit()
            {
                i += 1;
                while i < code.len() && char_at(code, i).is_ascii_digit() {
                    i += 1;
                }
            }
            push_token(&mut tokens, code, start, i, line, line);
            continue;
        }

        let start = i;
        let start_line = line;
        while i < code.len() {
            if code[i..].starts_with("--") || code[i..].starts_with("{-") {
                break;
            }
            let ch = char_at(code, i);
            if ch.is_whitespace()
                || ch == '"'
                || ch == '{'
                || ch == '-'
                || is_ident_start(ch)
                || ch.is_ascii_digit()
            {
                break;
            }
            i += ch.len_utf8();
        }
        if i == start {
            i += char_at(code, i).len_utf8();
        }
        push_token(&mut tokens, code, start, i, start_line, line);
    }

    tokens
}

pub fn detect(source: &str, tokens: &[Token], min_tokens: usize, max_tokens: usize) -> Vec<Repeat> {
    let n = tokens.len();
    let k = min_tokens.max(2);
    let max_len = max_tokens.max(k);
    if n < k {
        return Vec::new();
    }

    let mut groups: HashMap<[u8; 32], Vec<usize>> = HashMap::new();
    for i in 0..=(n - k) {
        let hash = *hash_slice(&tokens[i..i + k]).as_bytes();
        groups.entry(hash).or_default().push(i);
    }

    let mut candidates: Vec<(Vec<usize>, usize)> = Vec::new();
    for positions in groups.values() {
        if positions.len() < 2 {
            continue;
        }
        let anchor = positions[0];

        let mut left = 0;
        while positions
            .iter()
            .all(|&p| p > left && tokens[p - left - 1].text == tokens[anchor - left - 1].text)
        {
            left += 1;
        }
        let mut right = 0;
        while positions.iter().all(|&p| {
            p + k + right < n && tokens[p + k + right].text == tokens[anchor + k + right].text
        }) {
            right += 1;
        }

        let starts: Vec<usize> = positions.iter().map(|&p| p - left).collect();
        let len = (k + left + right).min(max_len);
        candidates.push((starts, len));
    }

    candidates.sort();
    candidates.dedup();

    candidates.sort_by(|a, b| {
        let score = |(starts, len): &(Vec<usize>, usize)| *len * starts.len();
        score(b).cmp(&score(a)).then_with(|| a.0[0].cmp(&b.0[0]))
    });

    let mut accepted: Vec<(Vec<usize>, usize)> = Vec::new();
    for (starts, len) in candidates {
        let clashes = starts.iter().any(|&s| {
            accepted
                .iter()
                .any(|(acc, acc_len)| acc.iter().any(|&a| s < a + acc_len && a < s + len))
        });
        if !clashes {
            accepted.push((starts, len));
        }
    }

    accepted
        .into_iter()
        .map(|(starts, len)| {
            let spans: Vec<Span> = starts
                .iter()
                .map(|&s| {
                    let first = &tokens[s];
                    let last = &tokens[s + len - 1];
                    Span {
                        start_line: first.start_line,
                        end_line: last.end_line,
                        start_byte: first.start_byte,
                        end_byte: last.end_byte,
                    }
                })
                .collect();
            Repeat {
                hash: hash_slice(&tokens[starts[0]..starts[0] + len])
                    .to_hex()
                    .to_string(),
                code: source[spans[0].start_byte..spans[0].end_byte].to_string(),
                token_len: len,
                occurrences: starts.len(),
                spans,
            }
        })
        .collect()
}

fn push_token(
    tokens: &mut Vec<Token>,
    code: &str,
    start: usize,
    end: usize,
    start_line: usize,
    end_line: usize,
) {
    tokens.push(Token {
        text: code[start..end].to_string(),
        start_line,
        end_line,
        start_byte: start,
        end_byte: end,
    });
}

fn char_at(code: &str, i: usize) -> char {
    code[i..].chars().next().expect("index within bounds")
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

fn is_statement_name(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('d') else {
        return false;
    };
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

fn hash_slice(tokens: &[Token]) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    for token in tokens {
        hasher.update(token.text.as_bytes());
        hasher.update(&[0u8]);
    }
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(tokens: &[Token]) -> Vec<&str> {
        tokens.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn tokenize_strips_comments_and_statement_names() {
        let code = "d1 $ s \"bd sn\" -- kick\n{- block\n comment -}\nd2 $ s \"bd sn\"";
        let tokens = tokenize(code);
        assert_eq!(
            texts(&tokens),
            vec!["$", "s", "\"bd sn\"", "$", "s", "\"bd sn\""]
        );
        assert_eq!(tokens[0].start_line, 1);
        assert_eq!(tokens[3].start_line, 4);
    }

    #[test]
    fn tokenize_keeps_string_case_and_tracks_multiline_spans() {
        let code = "s \"BD\nsn\"\ns \"bd\"";
        let tokens = tokenize(code);
        assert_eq!(texts(&tokens), vec!["s", "\"BD\nsn\"", "s", "\"bd\""]);
        assert_eq!(tokens[1].start_line, 1);
        assert_eq!(tokens[1].end_line, 2);
        assert_eq!(tokens[3].start_line, 3);
    }

    #[test]
    fn detect_finds_a_repeated_statement_across_d1_d2() {
        let code = "d1 $ s \"bd sn\"\nd2 $ s \"bd sn\"";
        let tokens = tokenize(code);
        let repeats = detect(code, &tokens, 3, 64);
        assert_eq!(repeats.len(), 1);
        assert_eq!(repeats[0].token_len, 3);
        assert_eq!(repeats[0].occurrences, 2);
        assert_eq!(repeats[0].spans.len(), 2);
        assert_eq!(repeats[0].spans[0].start_line, 1);
        assert_eq!(repeats[0].spans[1].start_line, 2);
        assert_eq!(repeats[0].code, "$ s \"bd sn\"");
    }

    #[test]
    fn detect_prefers_the_longest_block() {
        let code = "a b c d e f g\nx\na b c d e f h";
        let tokens = tokenize(code);
        let repeats = detect(code, &tokens, 3, 64);
        assert_eq!(repeats.len(), 1);
        assert_eq!(repeats[0].token_len, 6);
        assert_eq!(repeats[0].occurrences, 2);
        assert_eq!(repeats[0].code, "a b c d e f");
    }

    #[test]
    fn detect_ignores_unique_code() {
        let code = "a b c d e f";
        let tokens = tokenize(code);
        assert!(detect(code, &tokens, 3, 64).is_empty());
    }

    #[test]
    fn detect_respects_the_minimum_length() {
        let code = "s \"bd\"\ns \"bd\"";
        let tokens = tokenize(code);
        assert!(detect(code, &tokens, 3, 64).is_empty());

        let repeats = detect(code, &tokens, 2, 64);
        assert_eq!(repeats.len(), 1);
        assert_eq!(repeats[0].token_len, 2);
        assert_eq!(repeats[0].occurrences, 2);
    }

    #[test]
    fn detect_ignores_comments_and_formatting() {
        let code = "d1 $ s \"bd\" -- first\n\n   d2 $ s \"bd\" {- tail -}";
        let tokens = tokenize(code);
        let repeats = detect(code, &tokens, 2, 64);
        assert_eq!(repeats.len(), 1);
        assert_eq!(repeats[0].occurrences, 2);
    }

    #[test]
    fn detect_treats_string_content_case_sensitively() {
        let code = "s \"BD\"\ns \"bd\"";
        let tokens = tokenize(code);
        assert!(detect(code, &tokens, 2, 64).is_empty());
    }

    #[test]
    fn repeat_hash_is_stable_and_content_dependent() {
        let a = "s \"bd\"\ns \"bd\"";
        let b = "s \"sn\"\ns \"sn\"";
        let ra = detect(a, &tokenize(a), 2, 64);
        let rb = detect(b, &tokenize(b), 2, 64);
        assert_eq!(ra.len(), 1);
        assert_eq!(rb.len(), 1);
        assert_ne!(ra[0].hash, rb[0].hash);
        assert_eq!(ra[0].hash, detect(a, &tokenize(a), 2, 64)[0].hash);
    }

    #[test]
    fn detect_caps_the_block_length() {
        let code = "a b c d e f g h\na b c d e f g i";
        let tokens = tokenize(code);
        let repeats = detect(code, &tokens, 3, 5);
        assert_eq!(repeats.len(), 1);
        assert_eq!(repeats[0].token_len, 5);
    }
}
