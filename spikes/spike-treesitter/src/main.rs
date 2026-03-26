mod adapters;
mod parser;

use adapters::{python::PythonAdapter, rust::RustAdapter, typescript::TypeScriptAdapter};
use anyhow::Result;
use clap::Parser as ClapParser;
use parser::LanguageAdapter;
use snif_common::FileExtraction;
use std::path::Path;
use std::time::Instant;
use walkdir::WalkDir;

#[derive(ClapParser)]
#[command(name = "spike-treesitter")]
#[command(about = "Validate tree-sitter extraction for Rust, TypeScript, and Python")]
struct Cli {
    #[arg(long, help = "Path to a repository to parse")]
    repo: Option<String>,

    #[arg(long, help = "Output extracted data as JSON")]
    json: bool,
}

fn all_adapters() -> Vec<Box<dyn LanguageAdapter>> {
    vec![
        Box::new(RustAdapter),
        Box::new(TypeScriptAdapter::new(false)),
        Box::new(TypeScriptAdapter::new(true)),
        Box::new(PythonAdapter),
    ]
}

fn detect_adapter(path: &Path) -> Option<Box<dyn LanguageAdapter>> {
    let ext = path.extension()?.to_str()?;
    for adapter in all_adapters() {
        if adapter.file_extensions().contains(&ext) {
            return Some(adapter);
        }
    }
    None
}

fn parse_repo(repo_path: &str) -> Result<Vec<FileExtraction>> {
    let mut extractions = Vec::new();
    let mut total_files = 0;
    let mut total_imports = 0;
    let mut total_symbols = 0;
    let mut total_references = 0;
    let mut total_errors = 0;
    let mut files_by_lang: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    let start = Instant::now();

    for entry in WalkDir::new(repo_path).into_iter().filter_entry(|e| {
        let name = e.file_name().to_str().unwrap_or("");
        // Skip hidden dirs, target, node_modules, etc.
        !name.starts_with('.') && name != "target" && name != "node_modules" && name != "vendor"
    }) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let adapter = match detect_adapter(path) {
            Some(a) => a,
            None => continue,
        };

        let source = match std::fs::read(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Skip binary / very large files
        if source.len() > 1_000_000 || !is_likely_text(&source) {
            continue;
        }

        let rel_path = path.strip_prefix(repo_path).unwrap_or(path);
        let extraction =
            parser::parse_file(adapter.as_ref(), &rel_path.to_string_lossy(), &source)?;

        total_files += 1;
        total_imports += extraction.imports.len();
        total_symbols += extraction.symbols.len();
        total_references += extraction.references.len();
        total_errors += extraction.parse_errors.len();

        let lang = format!("{:?}", extraction.language);
        *files_by_lang.entry(lang).or_insert(0) += 1;

        extractions.push(extraction);
    }

    let elapsed = start.elapsed();

    println!("\n=== Tree-sitter Extraction Results ===\n");
    println!("  Repository: {}", repo_path);
    println!("  Parse time: {:?}", elapsed);
    println!("  Total files: {}", total_files);
    println!();
    for (lang, count) in &files_by_lang {
        println!("  {}: {} files", lang, count);
    }
    println!();
    println!("  Total imports:    {}", total_imports);
    println!("  Total symbols:    {}", total_symbols);
    println!("  Total references: {}", total_references);
    println!("  Total errors:     {}", total_errors);

    if total_files > 0 {
        println!(
            "\n  Speed: {:.0} files/sec",
            total_files as f64 / elapsed.as_secs_f64()
        );
        println!(
            "  Avg imports/file:    {:.1}",
            total_imports as f64 / total_files as f64
        );
        println!(
            "  Avg symbols/file:    {:.1}",
            total_symbols as f64 / total_files as f64
        );
        println!(
            "  Avg references/file: {:.1}",
            total_references as f64 / total_files as f64
        );
    }

    Ok(extractions)
}

fn run_builtin_test() -> Result<()> {
    println!("=== Spike 2: Tree-sitter Extraction Validation ===\n");

    // Test Rust parsing
    println!("--- Rust ---\n");
    let rust_source = br#"
use std::collections::HashMap;
use anyhow::{Context, Result};
use crate::parser::LanguageAdapter;

pub struct Config {
    pub name: String,
    pub value: i64,
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Processor {
    fn process(&self, input: &str) -> Result<String>;
}

pub fn parse_config(path: &str) -> Result<Config> {
    let data = std::fs::read_to_string(path)?;
    let map: HashMap<String, String> = serde_json::from_str(&data)?;
    Ok(Config {
        name: map.get("name").context("missing name")?.clone(),
        value: 42,
    })
}

fn helper() -> String {
    format!("hello")
}
"#;

    let adapter = RustAdapter;
    let extraction = parser::parse_file(&adapter, "test.rs", rust_source)?;
    print_extraction(&extraction);

    // Test TypeScript parsing
    println!("\n--- TypeScript ---\n");
    let ts_source = br#"
import { Router } from 'express';
import type { Request, Response } from 'express';
import { UserService } from './services/user';

interface UserData {
    id: string;
    name: string;
    email: string;
}

type UserRole = 'admin' | 'user' | 'guest';

class UserController {
    constructor(private service: UserService) {}

    async getUser(req: Request, res: Response) {
        const user = await this.service.findById(req.params.id);
        res.json(user);
    }
}

const createRouter = (controller: UserController): Router => {
    const router = Router();
    router.get('/:id', controller.getUser.bind(controller));
    return router;
};

export function setupRoutes(app: any) {
    const service = new UserService();
    const controller = new UserController(service);
    app.use('/users', createRouter(controller));
}
"#;

    let adapter = TypeScriptAdapter::new(false);
    let extraction = parser::parse_file(&adapter, "test.ts", ts_source)?;
    print_extraction(&extraction);

    // Test Python parsing
    println!("\n--- Python ---\n");
    let py_source = br#"
import os
import json
from pathlib import Path
from typing import Optional, List

class Config:
    def __init__(self, path: str):
        self.path = path
        self.data = {}

    def load(self) -> dict:
        with open(self.path) as f:
            self.data = json.load(f)
        return self.data

    def get(self, key: str, default=None):
        return self.data.get(key, default)

def parse_config(path: str) -> Config:
    config = Config(path)
    config.load()
    return config

def validate_path(path: str) -> Optional[Path]:
    p = Path(path)
    if p.exists():
        return p
    return None
"#;

    let adapter = PythonAdapter;
    let extraction = parser::parse_file(&adapter, "test.py", py_source)?;
    print_extraction(&extraction);

    Ok(())
}

fn print_extraction(e: &FileExtraction) {
    println!("  File: {} ({:?})", e.path, e.language);
    println!("  Imports ({}):", e.imports.len());
    for imp in &e.imports {
        if imp.names.is_empty() {
            println!("    L{}: {}", imp.line, imp.source);
        } else {
            println!(
                "    L{}: {} :: {}",
                imp.line,
                imp.source,
                imp.names.join(", ")
            );
        }
    }
    println!("  Symbols ({}):", e.symbols.len());
    for sym in &e.symbols {
        println!(
            "    L{}-{}: {:?} {}",
            sym.start_line, sym.end_line, sym.kind, sym.name
        );
    }
    println!("  References ({}):", e.references.len());
    for r in e.references.iter().take(10) {
        println!("    L{}: {}", r.line, r.name);
    }
    if e.references.len() > 10 {
        println!("    ... and {} more", e.references.len() - 10);
    }
    if !e.parse_errors.is_empty() {
        println!("  Errors ({}):", e.parse_errors.len());
        for err in &e.parse_errors {
            println!("    L{}:{}: {}", err.line, err.column, err.message);
        }
    }
}

fn is_likely_text(data: &[u8]) -> bool {
    // Quick heuristic: check first 512 bytes for null bytes
    let check_len = data.len().min(512);
    !data[..check_len].contains(&0)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Always run built-in tests first
    run_builtin_test()?;

    // If a repo path is provided, parse it
    if let Some(repo_path) = &cli.repo {
        println!("\n\n=== Parsing Repository ===");
        let extractions = parse_repo(repo_path)?;

        if cli.json {
            let json = serde_json::to_string_pretty(&extractions)?;
            let output_path = "/tmp/snif_spike_treesitter_output.json";
            std::fs::write(output_path, &json)?;
            println!("\n  JSON output written to: {}", output_path);
        }
    }

    println!("\n=== Spike 2 Complete ===");
    Ok(())
}
