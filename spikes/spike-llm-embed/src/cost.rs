use std::time::Duration;

pub struct CostReport {
    pub total_units: usize,
    pub total_input_chars: usize,
    pub total_output_chars: usize,
    pub total_summarization_time: Duration,
    pub total_embedding_time: Duration,
    pub avg_summary_time: Duration,
    pub estimated_input_tokens: usize,
    pub estimated_output_tokens: usize,
}

impl CostReport {
    pub fn new(
        total_units: usize,
        total_input_chars: usize,
        total_output_chars: usize,
        total_summarization_time: Duration,
        total_embedding_time: Duration,
    ) -> Self {
        let avg_summary_time = if total_units > 0 {
            total_summarization_time / total_units as u32
        } else {
            Duration::ZERO
        };

        // Rough token estimation: ~4 chars per token for English/code mix
        let estimated_input_tokens = total_input_chars / 4;
        let estimated_output_tokens = total_output_chars / 4;

        Self {
            total_units,
            total_input_chars,
            total_output_chars,
            total_summarization_time,
            total_embedding_time,
            avg_summary_time,
            estimated_input_tokens,
            estimated_output_tokens,
        }
    }

    pub fn print(&self) {
        println!("\n  --- Cost Report ---\n");
        println!("  Code units summarized:  {}", self.total_units);
        println!("  Total input chars:      {}", self.total_input_chars);
        println!("  Total output chars:     {}", self.total_output_chars);
        println!(
            "  Est. input tokens:      {}",
            self.estimated_input_tokens
        );
        println!(
            "  Est. output tokens:     {}",
            self.estimated_output_tokens
        );
        println!(
            "  Total summarization:    {:?}",
            self.total_summarization_time
        );
        println!(
            "  Avg per summary:        {:?}",
            self.avg_summary_time
        );
        println!(
            "  Total embedding time:   {:?}",
            self.total_embedding_time
        );

        // Cost estimate at Claude Sonnet pricing (Bedrock)
        // ~$3/1M input tokens, ~$15/1M output tokens
        let input_cost = self.estimated_input_tokens as f64 / 1_000_000.0 * 3.0;
        let output_cost = self.estimated_output_tokens as f64 / 1_000_000.0 * 15.0;
        let total_cost = input_cost + output_cost;
        println!("\n  Est. cost (Sonnet):     ${:.4}", total_cost);

        // Extrapolate
        if self.total_units > 0 {
            let cost_per_unit = total_cost / self.total_units as f64;
            println!("  Cost per unit:          ${:.6}", cost_per_unit);
            println!(
                "  Est. for 1k units:      ${:.2}",
                cost_per_unit * 1000.0
            );
            println!(
                "  Est. for 5k units:      ${:.2}",
                cost_per_unit * 5000.0
            );
            println!(
                "  Est. for 10k units:     ${:.2}",
                cost_per_unit * 10000.0
            );

            let time_per_unit = self.total_summarization_time.as_secs_f64() / self.total_units as f64;
            println!(
                "\n  Est. time for 1k units: {:.0}m",
                time_per_unit * 1000.0 / 60.0
            );
            println!(
                "  Est. time for 5k units: {:.0}m",
                time_per_unit * 5000.0 / 60.0
            );
        }
    }
}
