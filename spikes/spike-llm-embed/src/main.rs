mod cost;
mod embedder;
mod extractor;
mod quality;
mod store;
mod summarizer;

use anyhow::Result;
use clap::Parser;
use cost::CostReport;
use std::time::Duration;
use zerocopy::AsBytes;

#[derive(Parser)]
#[command(name = "spike-llm-embed")]
#[command(about = "Validate LLM summarization + embedding pipeline")]
struct Cli {
    #[arg(long, default_value = "/tmp/snif-test-repo-axum")]
    repo: String,

    #[arg(long, default_value = "20", help = "Max code units to summarize")]
    limit: usize,

    #[arg(
        long,
        default_value = "amazon-bedrock/anthropic.claude-haiku-4-5-20251001-v1:0",
        help = "Model for summarization (provider/model format)"
    )]
    model: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=== Spike 3: LLM Summarization + Embedding ===\n");
    println!("  Repository: {}", cli.repo);
    println!("  Model:      {}", cli.model);
    println!("  Limit:      {} code units\n", cli.limit);

    // Step 1: Extract code units from the repository
    println!("  Finding code units...");
    let units = extractor::find_rust_functions(&cli.repo, cli.limit)?;
    println!("  Found {} code units\n", units.len());

    if units.is_empty() {
        println!("  No code units found. Check the repo path.");
        return Ok(());
    }

    // Step 2: Set up the database
    store::init_sqlite_vec();
    let db_path = "/tmp/snif_spike_llm_embed.db";
    let conn = store::create_db(db_path)?;

    let mut insert_stmt = conn.prepare(
        "INSERT INTO code_summaries (name, file_path, body) VALUES (?1, ?2, ?3)",
    )?;
    for unit in &units {
        insert_stmt.execute(rusqlite::params![unit.name, unit.file_path, unit.body])?;
    }
    drop(insert_stmt);

    // Step 3: Summarize each code unit via the LLM provider
    println!("  --- Summarizing code units ---\n");
    let mut total_input_chars = 0usize;
    let mut total_output_chars = 0usize;
    let mut total_summary_time = Duration::ZERO;
    let mut summaries: Vec<String> = Vec::new();

    for (i, unit) in units.iter().enumerate() {
        print!("  [{}/{}] {} ... ", i + 1, units.len(), unit.name);

        match summarizer::summarize_code_unit(&unit.body, &unit.file_path, &unit.name, &cli.model)
        {
            Ok(result) => {
                let display: String = result.summary.chars().take(80).collect();
                println!("OK ({:?}) - {}", result.duration, display);

                total_input_chars += result.input_chars;
                total_output_chars += result.output_chars;
                total_summary_time += result.duration;

                conn.execute(
                    "UPDATE code_summaries SET summary = ?1, input_chars = ?2,
                     output_chars = ?3, summary_time_ms = ?4 WHERE id = ?5",
                    rusqlite::params![
                        result.summary,
                        result.input_chars,
                        result.output_chars,
                        result.duration.as_millis() as i64,
                        (i + 1) as i64,
                    ],
                )?;

                summaries.push(result.summary);
            }
            Err(e) => {
                println!("FAILED: {}", e);
                summaries.push(String::new());
            }
        }
    }

    let successful = summaries.iter().filter(|s| !s.is_empty()).count();
    println!(
        "\n  Summarized {}/{} code units successfully",
        successful,
        units.len()
    );

    // Step 4: Embed summaries locally
    println!("\n  --- Embedding summaries ---\n");
    let embedder = embedder::Embedder::new()?;

    let non_empty_summaries: Vec<String> = summaries
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();

    let embed_result = embedder.embed_batch(&non_empty_summaries)?;
    println!(
        "  Embedded {} summaries in {:?} (dim={})",
        embed_result.embeddings.len(),
        embed_result.duration,
        embed_result.dimension
    );

    let mut embed_idx = 0;
    let mut embed_stmt = conn.prepare(
        "INSERT INTO summary_embeddings (summary_id, embedding) VALUES (?1, ?2)",
    )?;
    for (i, summary) in summaries.iter().enumerate() {
        if !summary.is_empty() {
            let embedding = &embed_result.embeddings[embed_idx];
            embed_stmt.execute(rusqlite::params![
                (i + 1) as i64,
                embedding.as_bytes()
            ])?;
            embed_idx += 1;
        }
    }
    drop(embed_stmt);

    // Step 5: Cost report
    let report = CostReport::new(
        successful,
        total_input_chars,
        total_output_chars,
        total_summary_time,
        embed_result.duration,
    );
    report.print();

    // Step 6: Quality evaluation
    println!("\n  --- Embedding Quality Evaluation ---\n");

    let sample_count = units.len().min(5);
    let mut results = Vec::new();

    for i in 0..sample_count {
        if summaries[i].is_empty() {
            continue;
        }

        let query_embedding = embedder.embed_single(&summaries[i])?;
        let matches = quality::query_similar(&conn, &query_embedding, 6)?;

        let filtered: Vec<_> = matches
            .into_iter()
            .filter(|m| m.name != units[i].name || m.file_path != units[i].file_path)
            .take(5)
            .collect();

        results.push(quality::SimilarityResult {
            query_name: units[i].name.clone(),
            query_file: units[i].file_path.clone(),
            matches: filtered,
        });
    }

    quality::print_similarity_results(&results);

    // Step 7: Sample summaries
    println!("  --- Sample Summaries ---\n");
    for (i, (unit, summary)) in units.iter().zip(summaries.iter()).enumerate().take(5) {
        if summary.is_empty() {
            continue;
        }
        println!("  {}. {} ({})", i + 1, unit.name, unit.file_path);
        println!("     {}\n", summary);
    }

    println!("=== Spike 3 Complete ===");
    Ok(())
}
