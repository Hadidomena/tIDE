pub const TOP_PROMPT: &str = "tIDE> ";
pub const CONT_PROMPT: &str = "tIDE| ";
pub const PROMPT_SETUP: &str = ":set prompt \"tIDE> \"\n:set prompt-cont \"tIDE| \"\n";

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
}
