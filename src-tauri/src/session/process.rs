pub const TOP_PROMPT: &str = "tIDE> ";
pub const CONT_PROMPT: &str = "tIDE| ";
pub const PROMPT_SETUP: &str = ":set prompt \"tIDE> \"\n\
:set prompt-cont \"tIDE| \"\n\
:set -XOverloadedStrings\n\
:set -XExtendedDefaultRules\n";

pub const PRELUDE: &str = "import Sound.Tidal.Context\n\
tidal <- startTidal superdirtTarget defaultConfig\n\
let d1 = streamReplace tidal 1\n\
let d2 = streamReplace tidal 2\n\
let d3 = streamReplace tidal 3\n\
let d4 = streamReplace tidal 4\n\
let d5 = streamReplace tidal 5\n\
let d6 = streamReplace tidal 6\n\
let d7 = streamReplace tidal 7\n\
let d8 = streamReplace tidal 8\n\
let hush = streamHush tidal\n";

pub fn prelude_prompts() -> usize {
    PRELUDE.matches('\n').count()
}

pub fn split_statements(source: &str) -> Vec<String> {
    let mut statements: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    let mut in_block = false;

    for raw in source.replace("\r\n", "\n").lines() {
        let trimmed = raw.trim();
        if in_block {
            if let Some(cur) = current.as_mut() {
                cur.push('\n');
                cur.push_str(raw);
            }
            if trimmed == ":}" {
                in_block = false;
                if let Some(cur) = current.take() {
                    statements.push(cur);
                }
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        if trimmed.starts_with(":{") {
            if let Some(cur) = current.take() {
                statements.push(cur);
            }
            current = Some(raw.to_string());
            in_block = true;
            continue;
        }
        let indented = raw.starts_with(' ') || raw.starts_with('\t');
        if indented {
            if let Some(cur) = current.as_mut() {
                cur.push('\n');
                cur.push_str(raw);
            } else {
                current = Some(raw.to_string());
            }
        } else {
            if let Some(cur) = current.take() {
                statements.push(cur);
            }
            current = Some(raw.to_string());
        }
    }
    if let Some(cur) = current {
        statements.push(cur);
    }
    statements
        .into_iter()
        .map(|statement| statement.trim_end().to_string())
        .filter(|statement| !statement.trim().is_empty())
        .collect()
}

pub fn wrap_code(code: &str) -> String {
    let code = code.trim();
    if code.is_empty() {
        return String::new();
    }
    if code.starts_with(':') {
        format!("{code}\n")
    } else {
        format!(":{{\n{code}\n:}}\n")
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
        assert_eq!(wrap_code(":reload"), ":reload\n");
        assert_eq!(wrap_code(" :t map "), ":t map\n");
    }

    #[test]
    fn plain_code_gets_block_framing() {
        assert_eq!(wrap_code("1+1"), ":{\n1+1\n:}\n");
        assert_eq!(
            wrap_code("let x = [1,2\n     ]"),
            ":{\nlet x = [1,2\n     ]\n:}\n"
        );
    }

    #[test]
    fn empty_code_produces_nothing() {
        assert_eq!(wrap_code("   \n  "), "");
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
        assert!(PRELUDE.contains("tidal <- startTidal superdirtTarget defaultConfig\n"));
        for name in ["d1", "d8", "hush"] {
            assert!(
                PRELUDE.contains(&format!("let {name} = ")),
                "{name} binding missing"
            );
        }
        assert_eq!(prelude_prompts(), 11);
        assert!(!PRELUDE.contains(":{"));
    }

    #[test]
    fn splits_file_into_statements() {
        let source =
            "-- header\n\nd1 $ s \"bd\"\nd2 $ s \"sn\" -- tail\n\n-- middle\nd3 $ s \"hh\"\n";
        assert_eq!(
            split_statements(source),
            vec![
                "d1 $ s \"bd\"".to_string(),
                "d2 $ s \"sn\" -- tail".to_string(),
                "d3 $ s \"hh\"".to_string()
            ]
        );
    }

    #[test]
    fn keeps_indented_continuations() {
        let source = "d1 $ s \"bd\"\n   # gain 0.8\n   # pan 0.2\nd2 $ s \"sn\"\n";
        assert_eq!(
            split_statements(source),
            vec![
                "d1 $ s \"bd\"\n   # gain 0.8\n   # pan 0.2".to_string(),
                "d2 $ s \"sn\"".to_string()
            ]
        );
    }

    #[test]
    fn keeps_explicit_blocks_whole() {
        let source = ":{\nlet x = 1\n    y = 2\n:}\nd1 $ s \"bd\"\n";
        assert_eq!(
            split_statements(source),
            vec![
                ":{\nlet x = 1\n    y = 2\n:}".to_string(),
                "d1 $ s \"bd\"".to_string()
            ]
        );
    }

    #[test]
    fn handles_crlf_and_empty_input() {
        assert_eq!(
            split_statements("d1 $ s \"bd\"\r\nd2 $ s \"sn\"\r\n"),
            vec!["d1 $ s \"bd\"".to_string(), "d2 $ s \"sn\"".to_string()]
        );
        assert!(split_statements("").is_empty());
        assert!(split_statements("\n-- only comments\n").is_empty());
    }
}
