pub const TOP_PROMPT: &str = "tIDE> ";
pub const CONT_PROMPT: &str = "tIDE| ";
pub const PROMPT_SETUP: &str = ":set prompt \"tIDE> \"\n\
:set prompt-cont \"tIDE| \"\n\
:set -XOverloadedStrings\n\
:set -XExtendedDefaultRules\n";

pub const PRELUDE: &str = "import Sound.Tidal.Context\n\
tidalInst <- startTidal superdirtTarget defaultConfig\n\
import Sound.Tidal.Boot\n\
instance Tidally where tidal = tidalInst\n";

pub fn prelude_prompts() -> usize {
    PRELUDE.matches('\n').count()
}

pub struct Statement {
    pub line: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhcDiagnostic {
    pub line: usize,
    pub column: Option<usize>,
    pub summary: String,
    pub details: Vec<String>,
}

pub fn parse_ghc_diagnostics(output: &str) -> Vec<GhcDiagnostic> {
    let output = strip_ansi(output).replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = output.lines().collect();
    let headers: Vec<(usize, usize, Option<usize>)> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            parse_error_header(line).map(|(line, column)| (index, line, column))
        })
        .collect();

    headers
        .iter()
        .enumerate()
        .filter_map(|(header_index, &(line_index, line, column))| {
            let end = headers
                .get(header_index + 1)
                .map_or(lines.len(), |(next_header, _, _)| *next_header);
            let mut content: Vec<&str> = lines[line_index + 1..end]
                .iter()
                .copied()
                .take_while(|line| !line.trim_start().starts_with(TOP_PROMPT))
                .collect();

            while content.first().is_some_and(|line| line.trim().is_empty()) {
                content.remove(0);
            }
            while content.last().is_some_and(|line| line.trim().is_empty()) {
                content.pop();
            }

            let summary_index = content.iter().position(|line| !line.trim().is_empty())?;
            let summary = clean_summary(content[summary_index]);
            if summary.is_empty() {
                return None;
            }

            let details = content
                .into_iter()
                .skip(summary_index + 1)
                .map(str::to_string)
                .collect();
            Some(GhcDiagnostic {
                line,
                column,
                summary,
                details,
            })
        })
        .collect()
}

fn parse_error_header(header: &str) -> Option<(usize, Option<usize>)> {
    let (file_start, _) = header
        .match_indices("tIDE.hs:")
        .find(|(start, _)| is_file_boundary(header, *start))?;
    let location = &header[file_start + "tIDE.hs:".len()..];
    let (line, mut rest) = parse_decimal_prefix(location)?;
    if line == 0 {
        return None;
    }

    let mut column = None;
    if let Some(after_colon) = rest.strip_prefix(':')
        && let Some((parsed_column, after_column)) = parse_decimal_prefix(after_colon)
    {
        if parsed_column == 0 {
            return None;
        }
        column = Some(parsed_column);
        rest = after_column;

        // Some GHC versions print a span as `line:column-endColumn`.
        if let Some(after_dash) = rest.strip_prefix('-') {
            let (end_column, after_end) = parse_decimal_prefix(after_dash)?;
            if end_column < parsed_column {
                return None;
            }
            rest = after_end;
        }
    }

    rest.strip_prefix(": error:")?;
    Some((line, column))
}

fn is_file_boundary(text: &str, start: usize) -> bool {
    start == 0
        || text[..start]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_whitespace() || ch == '/' || ch == '\\')
}

fn parse_decimal_prefix(text: &str) -> Option<(usize, &str)> {
    let digit_count = text.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0 {
        return None;
    }
    let value = text[..digit_count].parse().ok()?;
    Some((value, &text[digit_count..]))
}

fn clean_summary(line: &str) -> String {
    let summary = line.trim();
    summary
        .strip_prefix("• ")
        .or_else(|| summary.strip_prefix("- "))
        .unwrap_or(summary)
        .trim()
        .to_string()
}

fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('[') => {
                for code in chars.by_ref() {
                    if ('@'..='~').contains(&code) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(code) = chars.next() {
                    if code == '\x07' {
                        break;
                    }
                    if code == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    output
}

pub fn split_statements(source: &str) -> Vec<Statement> {
    let mut statements: Vec<Statement> = Vec::new();
    let mut current: Option<(usize, String)> = None;
    let mut in_block = false;

    for (index, raw) in source.replace("\r\n", "\n").lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if in_block {
            if let Some((_, cur)) = current.as_mut() {
                cur.push('\n');
                cur.push_str(raw);
            }
            if trimmed == ":}" {
                in_block = false;
                if let Some((start, text)) = current.take() {
                    statements.push(Statement { line: start, text });
                }
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        if trimmed.starts_with(":{") {
            if let Some((start, text)) = current.take() {
                statements.push(Statement { line: start, text });
            }
            current = Some((line, raw.to_string()));
            in_block = true;
            continue;
        }
        let indented = raw.starts_with(' ') || raw.starts_with('\t');
        if indented {
            if let Some((_, cur)) = current.as_mut() {
                cur.push('\n');
                cur.push_str(raw);
            } else {
                current = Some((line, raw.to_string()));
            }
        } else {
            if let Some((start, text)) = current.take() {
                statements.push(Statement { line: start, text });
            }
            current = Some((line, raw.to_string()));
        }
    }
    if let Some((start, text)) = current {
        statements.push(Statement { line: start, text });
    }
    statements
        .into_iter()
        .map(|mut statement| {
            statement.text = statement.text.trim_end().to_string();
            statement
        })
        .filter(|statement| !statement.text.trim().is_empty())
        .collect()
}

pub fn wrap_code(code: &str, line: Option<usize>) -> String {
    let code = code.trim();
    if code.is_empty() {
        return String::new();
    }
    if code.starts_with(':') {
        return format!("{code}\n");
    }
    match line {
        Some(line) => format!(":{{\n{{-# LINE {line} \"tIDE.hs\" #-}}\n{code}\n:}}\n"),
        None => format!(":{{\n{code}\n:}}\n"),
    }
}

pub fn is_done(acc: &str) -> bool {
    acc.ends_with(TOP_PROMPT)
}

pub fn is_ready(acc: &str, min_prompts: usize) -> bool {
    is_done(acc) && acc.matches(TOP_PROMPT).count() >= min_prompts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colon_commands_pass_through_unwrapped() {
        assert_eq!(wrap_code(":reload", None), ":reload\n");
        assert_eq!(wrap_code(" :t map ", Some(9)), ":t map\n");
    }

    #[test]
    fn plain_code_gets_block_framing() {
        assert_eq!(wrap_code("1+1", None), ":{\n1+1\n:}\n");
        assert_eq!(
            wrap_code("let x = [1,2\n     ]", None),
            ":{\nlet x = [1,2\n     ]\n:}\n"
        );
    }

    #[test]
    fn line_pragma_points_at_editor_line() {
        assert_eq!(
            wrap_code("undefinedThing", Some(42)),
            ":{\n{-# LINE 42 \"tIDE.hs\" #-}\nundefinedThing\n:}\n"
        );
    }

    #[test]
    fn empty_code_produces_nothing() {
        assert_eq!(wrap_code("   \n  ", None), "");
    }

    #[test]
    fn parses_a_multiline_ghc_diagnostic_with_a_column() {
        let output = concat!(
            "tIDE| tIDE.hs:7:22: error: [GHC-39999]\n",
            "    • No instance for ‘Fractional Int’ arising from the literal ‘2.0’\n",
            "      In the second argument of ‘(+)’, namely ‘2.0’\n",
            "      In the expression: (1 :: Int) + 2.0\n",
            "\n",
            "  |\n",
            "7 | let x = (1 :: Int) + 2.0\n",
            "  |                      ^\n",
            "\ntIDE> "
        );

        assert_eq!(
            parse_ghc_diagnostics(output),
            vec![GhcDiagnostic {
                line: 7,
                column: Some(22),
                summary: "No instance for ‘Fractional Int’ arising from the literal ‘2.0’"
                    .to_string(),
                details: vec![
                    "      In the second argument of ‘(+)’, namely ‘2.0’".to_string(),
                    "      In the expression: (1 :: Int) + 2.0".to_string(),
                    "".to_string(),
                    "  |".to_string(),
                    "7 | let x = (1 :: Int) + 2.0".to_string(),
                    "  |                      ^".to_string(),
                ],
            }]
        );
    }

    #[test]
    fn parses_multiple_diagnostics_and_a_missing_column() {
        let output = concat!(
            "tIDE.hs:3:4: error:\n",
            "    • First problem\n",
            "      More context\n",
            "\n",
            "tIDE.hs:9: error: [GHC-12345]\n",
            "    - Second problem\n",
            "tIDE> "
        );

        assert_eq!(
            parse_ghc_diagnostics(output),
            vec![
                GhcDiagnostic {
                    line: 3,
                    column: Some(4),
                    summary: "First problem".to_string(),
                    details: vec!["      More context".to_string()],
                },
                GhcDiagnostic {
                    line: 9,
                    column: None,
                    summary: "Second problem".to_string(),
                    details: vec![],
                },
            ]
        );
    }

    #[test]
    fn parses_gcc_style_column_spans_and_crlf_output() {
        let output = "tIDE.hs:4:8-10: error: problem\r\n    • Span diagnostic\r\ntIDE> ";
        let diagnostics = parse_ghc_diagnostics(output);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 4);
        assert_eq!(diagnostics[0].column, Some(8));
        assert_eq!(diagnostics[0].summary, "Span diagnostic");
    }

    #[test]
    fn ignores_non_error_output_and_unlocated_errors() {
        let output = concat!(
            "2\n",
            "<interactive>:1:1: error: not an editor location\n",
            "    • Ignore me\n",
            "tIDE.hs:2:4: warning: not an error\n",
            "    • Also ignore me\n",
            "tIDE> "
        );
        assert!(parse_ghc_diagnostics(output).is_empty());
    }

    #[test]
    fn strips_ansi_codes_from_diagnostic_text() {
        let output = "\x1b[31mtIDE.hs:2:3: error:\x1b[0m\n    • Bad type\ntIDE> ";
        let diagnostics = parse_ghc_diagnostics(output);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].summary, "Bad type");
    }

    #[test]
    fn completion_requires_exact_top_prompt_tail() {
        assert!(is_done("2\n<BLANK LINE>\ntIDE> "));
        assert!(!is_done("tIDE| still continuing\n"));
        assert!(!is_done("Prelude> ")); // default prompt must never signal done
    }

    #[test]
    fn startup_requires_full_prompt_count() {
        assert!(!is_ready("banner…ghci> tIDE> ", 2));
        assert!(is_ready("banner…ghci> tIDE> tIDE> ", 2));
        assert!(is_ready("tIDE> ", 1));
    }

    #[test]
    fn prompt_setup_enables_tidal_pragmas() {
        assert!(PROMPT_SETUP.contains(":set -XOverloadedStrings\n"));
        assert!(PROMPT_SETUP.contains(":set -XExtendedDefaultRules\n"));
    }

    #[test]
    fn prelude_is_statement_per_line() {
        assert!(PRELUDE.starts_with("import Sound.Tidal.Context\n"));
        assert!(PRELUDE.contains("tidalInst <- startTidal superdirtTarget defaultConfig\n"));
        assert!(PRELUDE.contains("import Sound.Tidal.Boot\n"));
        assert!(PRELUDE.contains("instance Tidally where tidal = tidalInst\n"));
        assert_eq!(prelude_prompts(), 4);
        assert!(!PRELUDE.contains(":{"));
    }

    fn statements(source: &str) -> Vec<(usize, String)> {
        split_statements(source)
            .into_iter()
            .map(|s| (s.line, s.text))
            .collect()
    }

    #[test]
    fn splits_file_into_statements() {
        let source =
            "-- header\n\nd1 $ s \"bd\"\nd2 $ s \"sn\" -- tail\n\n-- middle\nd3 $ s \"hh\"\n";
        assert_eq!(
            statements(source),
            vec![
                (3, "d1 $ s \"bd\"".to_string()),
                (4, "d2 $ s \"sn\" -- tail".to_string()),
                (7, "d3 $ s \"hh\"".to_string())
            ]
        );
    }

    #[test]
    fn keeps_indented_continuations() {
        let source = "d1 $ s \"bd\"\n   # gain 0.8\n   # pan 0.2\nd2 $ s \"sn\"\n";
        assert_eq!(
            statements(source),
            vec![
                (1, "d1 $ s \"bd\"\n   # gain 0.8\n   # pan 0.2".to_string()),
                (4, "d2 $ s \"sn\"".to_string())
            ]
        );
    }

    #[test]
    fn keeps_explicit_blocks_whole() {
        let source = ":{\nlet x = 1\n    y = 2\n:}\nd1 $ s \"bd\"\n";
        assert_eq!(
            statements(source),
            vec![
                (1, ":{\nlet x = 1\n    y = 2\n:}".to_string()),
                (5, "d1 $ s \"bd\"".to_string())
            ]
        );
    }

    #[test]
    fn handles_crlf_and_empty_input() {
        assert_eq!(
            statements("d1 $ s \"bd\"\r\nd2 $ s \"sn\"\r\n"),
            vec![
                (1, "d1 $ s \"bd\"".to_string()),
                (2, "d2 $ s \"sn\"".to_string())
            ]
        );
        assert!(split_statements("").is_empty());
        assert!(split_statements("\n-- only comments\n").is_empty());
    }
}
