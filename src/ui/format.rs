use std::collections::HashMap;

/// Parse and evaluate a format string with variable substitution and conditionals.
///
/// Supports:
/// - `#{var_name}` — replaced with the variable value, or empty string if missing
/// - `#{?condition,true_val,false_val}` — if `condition` variable is non-empty, use true_val, else false_val
pub fn format_string(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '#' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let token = read_until_matching_brace(&mut chars);
            let expanded = expand_token(&token, vars);
            result.push_str(&expanded);
        } else {
            result.push(ch);
        }
    }

    result
}

/// Read characters until a matching '}', handling nested #{...} tokens.
fn read_until_matching_brace(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut buf = String::new();
    let mut depth = 1u32;

    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                depth += 1;
                buf.push(ch);
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return buf;
                }
                buf.push(ch);
            }
            _ => buf.push(ch),
        }
    }
    buf
}

fn expand_token(token: &str, vars: &HashMap<String, String>) -> String {
    if let Some(rest) = token.strip_prefix('?') {
        // Conditional: ?condition,true_val,false_val
        let parts = split_conditional(rest);
        if parts.len() >= 3 {
            let condition = parts[0];
            let true_val = parts[1];
            let false_val = parts[2];
            let is_truthy = vars.get(condition).map(|v| !v.is_empty()).unwrap_or(false);
            let chosen = if is_truthy { true_val } else { false_val };
            // Recursively expand #{...} tokens in the chosen branch
            format_string(chosen, vars)
        } else {
            String::new()
        }
    } else {
        // Simple variable lookup
        vars.get(token).cloned().unwrap_or_default()
    }
}

/// Split a conditional string on commas, respecting nested #{...} blocks.
fn split_conditional(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_simple_variable() {
        let vars = make_vars(&[("name", "hello")]);
        assert_eq!(format_string("#{name}", &vars), "hello");
    }

    #[test]
    fn test_multiple_variables() {
        let vars = make_vars(&[("a", "foo"), ("b", "bar")]);
        assert_eq!(format_string("#{a} #{b}", &vars), "foo bar");
    }

    #[test]
    fn test_missing_variable_empty() {
        let vars = make_vars(&[]);
        assert_eq!(format_string("#{missing}", &vars), "");
    }

    #[test]
    fn test_no_tokens() {
        let vars = make_vars(&[]);
        assert_eq!(format_string("plain text", &vars), "plain text");
    }

    #[test]
    fn test_conditional_true() {
        let vars = make_vars(&[("mode", "copy")]);
        assert_eq!(format_string("#{?mode,[#{mode}],}", &vars), "[copy]");
    }

    #[test]
    fn test_conditional_false() {
        let vars = make_vars(&[]);
        assert_eq!(format_string("#{?mode,[#{mode}],normal}", &vars), "normal");
    }

    #[test]
    fn test_conditional_empty_string_is_falsy() {
        let vars = make_vars(&[("mode", "")]);
        assert_eq!(format_string("#{?mode,yes,no}", &vars), "no");
    }

    #[test]
    fn test_mixed_text_and_variables() {
        let vars = make_vars(&[("cpu", "42%"), ("mem", "60%")]);
        assert_eq!(
            format_string("CPU #{cpu} | MEM #{mem}", &vars),
            "CPU 42% | MEM 60%"
        );
    }

    #[test]
    fn test_adjacent_tokens() {
        let vars = make_vars(&[("a", "x"), ("b", "y")]);
        assert_eq!(format_string("#{a}#{b}", &vars), "xy");
    }

    #[test]
    fn test_literal_hash_without_brace() {
        let vars = make_vars(&[]);
        assert_eq!(format_string("# not a token", &vars), "# not a token");
    }

    #[test]
    fn test_variable_with_surrounding_text() {
        let vars = make_vars(&[("pane_title", "zsh")]);
        assert_eq!(format_string(" #{pane_title} ", &vars), " zsh ");
    }

    // --- Nested conditionals ---

    #[test]
    fn test_nested_conditional_both_true() {
        let vars = make_vars(&[("a", "1"), ("b", "2")]);
        assert_eq!(format_string("#{?a,#{?b,x,y},z}", &vars), "x");
    }

    #[test]
    fn test_nested_conditional_outer_true_inner_false() {
        let vars = make_vars(&[("a", "1")]);
        assert_eq!(format_string("#{?a,#{?b,x,y},z}", &vars), "y");
    }

    #[test]
    fn test_nested_conditional_outer_false() {
        let vars = make_vars(&[("b", "2")]);
        assert_eq!(format_string("#{?a,#{?b,x,y},z}", &vars), "z");
    }

    // --- Malformed templates ---

    #[test]
    fn test_unclosed_brace_treated_as_var() {
        let vars = make_vars(&[]);
        // "#{missing" — no closing brace, read_until_matching_brace reads to end
        let result = format_string("#{missing", &vars);
        // The token is "missing" (read to EOF), which is looked up as a variable
        assert_eq!(result, "");
    }

    #[test]
    fn test_hash_without_open_brace() {
        let vars = make_vars(&[]);
        assert_eq!(format_string("#abc", &vars), "#abc");
    }

    #[test]
    fn test_empty_token() {
        let vars = make_vars(&[]);
        // #{} — empty token, looked up as "" variable
        assert_eq!(format_string("#{}", &vars), "");
    }

    // --- Conditional with fewer than 3 parts ---

    #[test]
    fn test_conditional_one_part_returns_empty() {
        let vars = make_vars(&[("a", "1")]);
        // #{?a} — only 1 part (the condition), no true/false branches
        assert_eq!(format_string("#{?a}", &vars), "");
    }

    #[test]
    fn test_conditional_two_parts_returns_empty() {
        let vars = make_vars(&[("a", "1")]);
        // #{?a,yes} — only 2 parts, needs 3
        assert_eq!(format_string("#{?a,yes}", &vars), "");
    }

    // --- Empty condition name ---

    #[test]
    fn test_conditional_empty_condition_name() {
        let vars = make_vars(&[]);
        // #{?,yes,no} — empty condition name, not in vars → falsy
        assert_eq!(format_string("#{?,yes,no}", &vars), "no");
    }

    #[test]
    fn test_conditional_empty_condition_with_empty_string_var() {
        let vars = make_vars(&[("", "")]);
        // Empty key exists but is empty string → falsy
        assert_eq!(format_string("#{?,yes,no}", &vars), "no");
    }

    #[test]
    fn test_conditional_empty_condition_with_nonempty_var() {
        let vars = make_vars(&[("", "val")]);
        // Empty key exists and is non-empty → truthy
        assert_eq!(format_string("#{?,yes,no}", &vars), "yes");
    }

    // --- Nested variable in conditional branches ---

    #[test]
    fn test_conditional_with_nested_var_in_true_branch() {
        let vars = make_vars(&[("flag", "1"), ("val", "hello")]);
        assert_eq!(format_string("#{?flag,#{val},default}", &vars), "hello");
    }

    #[test]
    fn test_conditional_with_nested_var_in_false_branch() {
        let vars = make_vars(&[("val", "hello")]);
        assert_eq!(format_string("#{?flag,default,#{val}}", &vars), "hello");
    }
}
