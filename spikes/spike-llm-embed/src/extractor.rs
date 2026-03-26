use anyhow::Result;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct CodeUnit {
    pub name: String,
    pub file_path: String,
    pub body: String,
}

pub fn find_rust_functions(repo_path: &str, limit: usize) -> Result<Vec<CodeUnit>> {
    let mut units = Vec::new();

    for entry in WalkDir::new(repo_path)
        .into_iter()
        .filter_entry(|e: &walkdir::DirEntry| {
            let name = e.file_name().to_str().unwrap_or("");
            !name.starts_with('.') && name != "target" && name != "node_modules"
        })
    {
        let entry: walkdir::DirEntry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "rs") {
            continue;
        }

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let rel_path = path
            .strip_prefix(repo_path)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        for (i, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if (trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub async fn ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("async fn "))
                && !trimmed.starts_with("fn main")
            {
                let name = extract_fn_name(trimmed);
                if name.is_empty() || name.starts_with('_') {
                    continue;
                }

                let end = (i + 30).min(source.lines().count());
                let body: String = source
                    .lines()
                    .skip(i)
                    .take(end - i)
                    .collect::<Vec<_>>()
                    .join("\n");

                if body.len() > 50 && body.len() < 3000 {
                    units.push(CodeUnit {
                        name,
                        file_path: rel_path.clone(),
                        body,
                    });
                }

                if units.len() >= limit {
                    return Ok(units);
                }
            }
        }
    }

    Ok(units)
}

fn extract_fn_name(line: &str) -> String {
    let line = line.trim();
    let after_fn = if let Some(pos) = line.find("fn ") {
        &line[pos + 3..]
    } else {
        return String::new();
    };

    after_fn
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}
