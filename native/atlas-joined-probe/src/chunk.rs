pub fn chunks(text: &str, target: usize) -> Vec<serde_json::Value> {
    let chars: Vec<char> = text.chars().collect();
    // Detect Markdown boundaries on the original characters, preserving offsets.
    let mut heading = vec![false; chars.len() + 1];
    let mut in_code = vec![false; chars.len() + 1];
    let mut paragraph_end = vec![false; chars.len() + 1];
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
        if !is_fence && trimmed.trim().is_empty() { paragraph_end[(end + 1).min(chars.len())] = true; }
        if !is_fence && indent <= 3 && trimmed.starts_with('#') {
            let n = trimmed.chars().take_while(|&c| c == '#').count();
            if n <= 6 && trimmed.chars().nth(n).is_some_and(char::is_whitespace) { heading[line] = true; }
        }
        if !is_fence && indent <= 3 && !trimmed.is_empty() && trimmed.chars().all(|c| c == '=' || c == '-') {
            if let Some(prev) = previous_line { heading[prev] = true; }
        }
        for value in &mut in_code[line..end] { *value = is_fence; }
        if end < chars.len() { in_code[end] = is_fence; }
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
                if heading[i] { section = Some(i); }
                if paragraph_end[i] { paragraph = Some(i); }
                if !in_code[i] && matches!(chars[i-1], '.' | '!' | '?') && chars[i].is_whitespace() { sentence = Some(i + 1); }
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
