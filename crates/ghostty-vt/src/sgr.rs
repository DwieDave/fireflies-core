//! Lines with their colors and styles as SGR alone, each line on its own:
//! what `read --ansi` gives. A line starts in the default style, ends with a
//! reset when it set any, and carries no other escape sequence. Consecutive
//! SGR sequences are merged, a reset followed by a style that keeps the
//! current one is written as what it adds, and the 16 palette colors get
//! their classic codes (31, 91, 41, 101).

/// Tidies one line of libghostty's VT output (see the module doc).
pub fn tidy(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    // Attributes set since the last text, not yet written.
    let mut pending: Vec<String> = Vec::new();
    // The attributes written since the last reset: the style now.
    let mut current: Vec<String> = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    let mut last = None;
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            last = Some(c);
                            break;
                        }
                        params.push(c);
                    }
                    if last == Some('m') {
                        pending.extend(groups(&params));
                    }
                }
                // Strings (OSC, DCS, APC, PM, SOS) end with BEL or ST.
                Some(']' | 'P' | '_' | '^' | 'X') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if c.is_control() && c != '\t' {
            continue;
        }
        if !pending.is_empty() {
            let set: Vec<String> = match pending.iter().rposition(|g| g == "0") {
                // A reset, then a style that keeps all of the current one: only what it adds.
                Some(at) if current.iter().all(|g| pending[at + 1..].contains(g)) => {
                    let target = pending[at + 1..].to_vec();
                    let added = target.iter().filter(|g| !current.contains(g)).cloned().collect();
                    current = target;
                    added
                }
                Some(at) => {
                    let mut set = vec!["0".to_string()];
                    set.extend_from_slice(&pending[at + 1..]);
                    current = set[1..].to_vec();
                    set
                }
                None => {
                    current.extend_from_slice(&pending);
                    std::mem::take(&mut pending)
                }
            };
            if !set.is_empty() {
                out.push_str(&format!("\x1b[{}m", set.join(";")));
            }
            pending.clear();
        }
        out.push(c);
    }
    if !current.is_empty() {
        out.push_str("\x1b[0m");
    }
    out
}

/// A line of `tidy` without its SGR: its text.
pub fn strip(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find('\x1b') {
        text.push_str(&rest[..at]);
        let seq = &rest[at..];
        let len =
            seq.char_indices().skip(2).find(|(_, c)| ('\x40'..='\x7e').contains(c)).map_or(seq.len(), |(i, _)| i + 1);
        rest = &seq[len.min(seq.len())..];
    }
    text.push_str(rest);
    text
}

/// One SGR's parameters as attributes: `38;5;n` and `38;2;r;g;b` stay
/// together, an empty parameter is a reset, palette colors 0-15 get their
/// classic codes.
fn groups(params: &str) -> Vec<String> {
    let tokens: Vec<&str> = params.split(';').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        let take = match (token, tokens.get(i + 1)) {
            ("38" | "48" | "58", Some(&"5")) => 3,
            ("38" | "48" | "58", Some(&"2")) => 5,
            _ => 1,
        };
        let group = &tokens[i..(i + take).min(tokens.len())];
        i += take;
        let palette = |base: u8, bright: u8| -> Option<String> {
            let n: u8 = group.get(2)?.parse().ok()?;
            match n {
                0..=7 => Some((base + n).to_string()),
                8..=15 => Some((bright + n - 8).to_string()),
                _ => None,
            }
        };
        let classic = match (group, take) {
            (["38", ..], 3) => palette(30, 90),
            (["48", ..], 3) => palette(40, 100),
            _ => None,
        };
        let reset = take == 1 && (token.is_empty() || token.parse::<u32>() == Ok(0));
        out.push(if reset { "0".into() } else { classic.unwrap_or_else(|| group.join(";")) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_line_stands_alone_in_as_few_sequences_as_it_takes() {
        // What libghostty writes for a red word, then bold 208 (all of it reset first).
        let line = "\x1b[0m\x1b[38;5;1mred\x1b[0m and \x1b[0m\x1b[1m\x1b[38;5;208mbold208\x1b[0m";
        assert_eq!(tidy(line), "\x1b[31mred\x1b[0m and \x1b[1;38;5;208mbold208\x1b[0m");
        assert_eq!(
            tidy("\x1b[0m\x1b[38;2;0;0;0mtrue\x1b[0m\x1b[38;2;0;0;0m\x1b[48;5;12m  \x1b[0m"),
            "\x1b[38;2;0;0;0mtrue\x1b[104m  \x1b[0m",
            "a 0 inside a color is no reset; the color stays"
        );
        assert_eq!(
            tidy("\x1b[0m\x1b[1m\x1b[31ma\x1b[0m\x1b[32mb"),
            "\x1b[1;31ma\x1b[0;32mb\x1b[0m",
            "dropping bold takes a reset"
        );
        assert_eq!(tidy("plain"), "plain");
        assert_eq!(tidy("\x1b[0m\x1b[1m"), "", "a style with no text after it");
        assert_eq!(tidy("\x1b[4:3m\x1b[58;2;1;2;3mcurly"), "\x1b[4:3;58;2;1;2;3mcurly\x1b[0m", "reset at the end");
        assert_eq!(tidy("\x1b[1mbold\x1b[mplain"), "\x1b[1mbold\x1b[0mplain");
        assert_eq!(tidy("\x1b[38;5;9mbright\x1b[48;5;7mon grey"), "\x1b[91mbright\x1b[47mon grey\x1b[0m");
        assert_eq!(tidy("a\x1b]8;;http://x\x1b\\link\x1b]8;;\x07\x1b[2Kb\x07\tc"), "alinkb\tc", "SGR alone");
    }

    #[test]
    fn stripping_gives_the_text_back() {
        assert_eq!(strip("\x1b[1;38;5;208mbold\x1b[0m and \x1b[31mred\x1b[0m"), "bold and red");
        assert_eq!(strip("\x1b[31"), "");
    }
}
