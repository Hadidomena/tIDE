pub const TOP_PROMPT: &str = "tIDE> ";
pub const CONT_PROMPT: &str = "tIDE| ";
/// Prompt + the two language pragmas the canonical Tidal boot file carries. Without
/// `OverloadedStrings`, `s "bd"` fails as `[Char]` vs `Pattern String`; without
/// `ExtendedDefaultRules`, numeric literals in patterns don't default to the Tidal types.
pub const PROMPT_SETUP: &str = ":set prompt \"tIDE> \"\n\
:set prompt-cont \"tIDE| \"\n\
:set -XOverloadedStrings\n\
:set -XExtendedDefaultRules\n";

/// Session prelude — verified verbatim in the M2 step-0 spike (tidal 1.10.3).
///
/// `startTidal` binds **nothing**: `d1..d8`/`hush` need explicit `let` bindings via
/// `streamReplace`/`streamHush`. Sent as **separate GHCi statements**, never as one
/// `:{ … :}` block — GHCi's multi-line mode rejects mixed import + `x <- io` lines.
/// Each statement produces exactly one prompt echo, so startup uses
/// [`prelude_prompts`] with the same prompt-count pump as [`PROMPT_SETUP`].
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

/// Number of prompt echoes the prelude produces (one per statement).
pub fn prelude_prompts() -> usize {
    PRELUDE.matches('\n').count()
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
        // 11 statements: import, tidal, d1..d8, hush — one prompt echo each.
        assert_eq!(prelude_prompts(), 11);
        assert!(!PRELUDE.contains(":{"));
    }
}
