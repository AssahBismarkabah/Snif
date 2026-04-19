pub mod formatting {
    pub const DOUBLE_NEWLINE: &str = "\n\n";
    pub const SINGLE_NEWLINE: &str = "\n";
    pub const BOT_MARKER: &str = "[snif-bot]";
    pub const MARKDOWN_CODE_BLOCK_OPEN: &str = "```";
    pub const MARKDOWN_CODE_BLOCK_CLOSE: &str = "\n```";
    pub const MARKDOWN_BOLD_OPEN: &str = "**";
    pub const MARKDOWN_BOLD_CLOSE: &str = "**";
    pub const MARKDOWN_ITALIC_OPEN: &str = "*";
    pub const MARKDOWN_ITALIC_CLOSE: &str = "*";
}

pub mod templates {
    pub const FINDING_BLOCK: &str = "{title} (confidence: {confidence:.0}%){suggestion}

{content}

**Impact:** {impact}

";
    pub const RESOLVED_MESSAGE: &str = "{marker}

**Resolved** — this issue is no longer present in the current change.";
}
