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
