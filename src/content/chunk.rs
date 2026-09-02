pub struct MarkdownChunk {
    pub markdown: String,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

pub fn slice(markdown: &str, offset: usize, max_chars: usize) -> MarkdownChunk {
    let characters: Vec<char> = markdown.chars().collect();
    let start = offset.min(characters.len());
    let end = (start + max_chars).min(characters.len());

    MarkdownChunk {
        markdown: characters[start..end].iter().collect(),
        offset: start,
        next_offset: (end < characters.len()).then_some(end),
    }
}
