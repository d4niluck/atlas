// Windows filename acceptance, kept separate so it can be tested without the backend.
pub fn valid_windows_filename(name: &str) -> bool {
    if name.is_empty() || name.ends_with([' ', '.']) || name.chars().any(|ch| ch <= '\u{1f}' || "<>:\"/\\|?*".contains(ch)) { return false; }
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ');
    if ["CONIN$", "CONOUT$"].iter().any(|reserved| name.eq_ignore_ascii_case(reserved)) { return false; }
    if ["CON", "PRN", "AUX", "NUL"].iter().any(|reserved| stem.eq_ignore_ascii_case(reserved)) { return false; }
    if let Some(prefix) = stem.get(..3) {
        if prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT") {
            if matches!(&stem[3..], "1"|"2"|"3"|"4"|"5"|"6"|"7"|"8"|"9"|"¹"|"²"|"³") { return false; }
        }
    }
    true
}


#[cfg(test)]
mod tests {
    use super::valid_windows_filename;
    #[test]
    fn rejects_windows_reserved_names_and_unsafe_characters() {
        for name in ["", ".", "..", "a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "bad\u{0001}x", "file.", "file ", "CON", "con.txt", "prn.log", "AUX", "Nul.tar.gz", "CONIN$", "conout$", "COM1", "com9.dat", "LPT1", "lpt9.md", "COM¹.txt", "com²", "LPT³.log"] {
            assert!(!valid_windows_filename(name), "accepted: {name:?}");
        }
    }
    #[test]
    fn accepts_valid_unicode_and_non_reserved_names() {
        for name in ["Заметки.md", "😀.txt", ".hidden", "COM0", "COM10", "LPT0", "LPT10", "CONSOLE.txt", "CONIN$.txt", "CONOUT$.txt", " leading.txt", "éclair.txt", "file name.md"] {
            assert!(valid_windows_filename(name), "rejected: {name:?}");
        }
    }
}
