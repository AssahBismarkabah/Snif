use std::io::{self, BufRead};

pub fn read_all_lines(reader: impl BufRead) -> io::Result<Vec<String>> {
    let mut lines = Vec::new();
    for line in reader.lines() {
        lines.push(line?);
    }
    Ok(lines)
}

const MAX_LINES: usize = 100_000;

pub fn read_lines_bounded(reader: impl BufRead) -> io::Result<Vec<String>> {
    let mut lines = Vec::with_capacity(1024);
    for line in reader.lines().take(MAX_LINES) {
        lines.push(line?);
    }
    Ok(lines)
}
