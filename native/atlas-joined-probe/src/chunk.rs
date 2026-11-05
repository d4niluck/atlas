pub fn chunks(text: &str, target: usize) -> Vec<serde_json::Value> {
    let chars: Vec<char> = text.chars().collect();
    // Detect Markdown boundaries on the original characters, preserving offsets.
    // Bits mark heading, code and paragraph boundaries without three full-size arrays.
    let mut flags = vec![0u8; chars.len() + 1];
    let mut fence: Option<(char, usize)> = None;
    let mut previous_line = None;
    let mut line = 0;
    while line < chars.len() {
        let end = (line..chars.len()).find(|&i| chars[i] == '\n').unwrap_or(chars.len());
        let content: String = chars[line..end].iter().collect();
        let content = content.trim_end_matches('\r');
        let indent = content.chars().take_while(|&c| c == ' ').count();
        let trimmed = &content[indent..];
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let count = marker.map_or(0, |m| trimmed.chars().take_while(|&c| c == m).count());
        let remainder = if count > 0 { &trimmed[count..] } else { trimmed };
        let valid_fence = indent <= 3 && count >= 3;
        let was_fenced = fence.is_some();
        if let Some((kind, length)) = fence {
            if valid_fence && marker == Some(kind) && count >= length && remainder.trim().is_empty() { fence = None; }
        } else if valid_fence && !(marker == Some('`') && remainder.contains('`')) {
            fence = Some((marker.unwrap(), count));
        }
        let is_indented_code = indent >= 4 || content.starts_with('\t');
        let is_fence = was_fenced || fence.is_some() || is_indented_code;
        if !is_fence && trimmed.trim().is_empty() { flags[(end + 1).min(chars.len())] |= 4; }
        if !is_fence && indent <= 3 && trimmed.starts_with('#') {
            let n = trimmed.chars().take_while(|&c| c == '#').count();
            if n <= 6 && trimmed.chars().nth(n).is_some_and(char::is_whitespace) { flags[line] |= 1; }
        }
        if !is_fence && indent <= 3 && !trimmed.is_empty() && trimmed.chars().all(|c| c == '=' || c == '-') {
            if let Some(prev) = previous_line { flags[prev] |= 1; }
        }
        if is_fence { for value in &mut flags[line..end] { *value |= 2; } }
        if end < chars.len() && is_fence { flags[end] |= 2; }
        previous_line = if !is_fence && !trimmed.is_empty() { Some(line) } else { None };
        line = end + 1;
    }
    let mut out = Vec::new();
    let mut start = 0;
    // 1200 is an experimental character ceiling, not a model token limit.
    let target = target.clamp(1, 1200);
    let mut utf16 = 0;
    while start < chars.len() {
        let limit = (start + target).min(chars.len());
        let mut end = limit;
        // Prefer structural boundaries in the latter half of the candidate.
        if limit < chars.len() {
            let floor = start + target / 2;
            let mut paragraph = None;
            let mut section = None;
            let mut sentence = None;
            let mut space = None;
            for i in floor.max(start + 1)..limit {
                if flags[i] & 1 != 0 { section = Some(i); }
                if flags[i] & 4 != 0 { paragraph = Some(i); }
                if flags[i] & 2 == 0 && matches!(chars[i-1], '.' | '!' | '?') && chars[i].is_whitespace() { sentence = Some(i + 1); }
                if chars[i].is_whitespace() { space = Some(i + 1); }
            }
            end = section.or(paragraph).or(sentence).or(space).unwrap_or(limit).min(limit);
        }
        if end < chars.len() && chars[end - 1] == '\r' && chars[end] == '\n' { end -= 1; }
        if end == start { end = (start + 2).min(chars.len()); }
        let end_utf16 = utf16 + chars[start..end].iter().map(|c| c.len_utf16()).sum::<usize>();
        out.push(serde_json::json!({"start":start,"end":end,"start_utf16":utf16,"end_utf16":end_utf16,"text":chars[start..end].iter().collect::<String>()}));
        utf16 = end_utf16;
        start = end;
    }
    out
}

#[cfg(test)] mod boundary_regression {
 use super::chunks;
 #[test] fn original_unicode_markdown_and_crlf_boundaries() {
  assert_eq!(chunks("😀 начало\r\n# Заголовок\r\nтекст. да\n\nподвал", 12), serde_json::from_str::<Vec<serde_json::Value>>(r###"[{"end":10,"end_utf16":11,"start":0,"start_utf16":0,"text":"😀 начало\r\n"},{"end":21,"end_utf16":22,"start":10,"start_utf16":11,"text":"# Заголовок"},{"end":30,"end_utf16":31,"start":21,"start_utf16":22,"text":"\r\nтекст. "},{"end":40,"end_utf16":41,"start":30,"start_utf16":31,"text":"да\n\nподвал"}]"###).unwrap());
  assert_eq!(chunks("один\nзаголовок\n======\nдва\n\nконец", 11), serde_json::from_str::<Vec<serde_json::Value>>(r###"[{"end":5,"end_utf16":5,"start":0,"start_utf16":0,"text":"один\n"},{"end":15,"end_utf16":15,"start":5,"start_utf16":5,"text":"заголовок\n"},{"end":26,"end_utf16":26,"start":15,"start_utf16":15,"text":"======\nдва\n"},{"end":32,"end_utf16":32,"start":26,"start_utf16":26,"text":"\nконец"}]"###).unwrap());
  assert_eq!(chunks("abc\n```rust\nfoo. bar\n```\n# итог\nтекст", 10), serde_json::from_str::<Vec<serde_json::Value>>(r###"[{"end":10,"end_utf16":10,"start":0,"start_utf16":0,"text":"abc\n```rus"},{"end":17,"end_utf16":17,"start":10,"start_utf16":10,"text":"t\nfoo. "},{"end":25,"end_utf16":25,"start":17,"start_utf16":17,"text":"bar\n```\n"},{"end":32,"end_utf16":32,"start":25,"start_utf16":25,"text":"# итог\n"},{"end":37,"end_utf16":37,"start":32,"start_utf16":32,"text":"текст"}]"###).unwrap());
  assert_eq!(chunks("a\r\nb\r\n🦊💻 конец", 3), serde_json::from_str::<Vec<serde_json::Value>>(r###"[{"end":3,"end_utf16":3,"start":0,"start_utf16":0,"text":"a\r\n"},{"end":6,"end_utf16":6,"start":3,"start_utf16":3,"text":"b\r\n"},{"end":9,"end_utf16":11,"start":6,"start_utf16":6,"text":"🦊💻 "},{"end":12,"end_utf16":14,"start":9,"start_utf16":11,"text":"кон"},{"end":14,"end_utf16":16,"start":12,"start_utf16":14,"text":"ец"}]"###).unwrap());
  assert_eq!(chunks("    код. пробел\n\tдругой? тест\nпосле", 10), serde_json::from_str::<Vec<serde_json::Value>>(r###"[{"end":9,"end_utf16":9,"start":0,"start_utf16":0,"text":"    код. "},{"end":17,"end_utf16":17,"start":9,"start_utf16":9,"text":"пробел\n\t"},{"end":25,"end_utf16":25,"start":17,"start_utf16":17,"text":"другой? "},{"end":35,"end_utf16":35,"start":25,"start_utf16":25,"text":"тест\nпосле"}]"###).unwrap());
 }
}
