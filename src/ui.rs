use console::{style, Emoji, Term};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub static CHECKMARK: Emoji<'_, '_> = Emoji("✓ ", "+ ");
pub static CROSS: Emoji<'_, '_> = Emoji("✗ ", "x ");
pub static ARROW: Emoji<'_, '_> = Emoji("→ ", "-> ");
pub static WARN: Emoji<'_, '_> = Emoji("⚠ ", "! ");

/// Section header: `=== title ===` in bold cyan
pub fn section(title: &str) -> String {
    format!("{}", style(format!("=== {title} ===")).bold().cyan())
}

/// Verdict for available protocol: green checkmark
pub fn verdict_available(protocol: &str, detail: &str) -> String {
    format!(
        "  {}{}: {}",
        CHECKMARK,
        style(protocol).green(),
        style(detail).green()
    )
}

/// Verdict for blocked protocol: red cross
pub fn verdict_blocked(protocol: &str, detail: &str) -> String {
    format!(
        "  {}{}: {}",
        CROSS,
        style(protocol).red(),
        style(format!("BLOCKED ({detail})")).red()
    )
}

/// Verdict for warning (suspicious redirect, etc): yellow warning
pub fn verdict_warning(protocol: &str, detail: &str) -> String {
    format!(
        "  {}{}: {}",
        WARN,
        style(protocol).yellow(),
        style(detail).yellow()
    )
}

/// "Blocked protocols: HTTP, ..." in red bold
pub fn blocked_list(protocols: &str) -> String {
    format!(
        "{} {}",
        style("Blocked protocols:").red().bold(),
        style(protocols).red().bold()
    )
}

/// Summary: N working strategies found — green bold
pub fn summary_found(protocol: &str, count: usize) -> String {
    format!(
        "  {}{}: {}",
        CHECKMARK,
        style(protocol).green().bold(),
        style(format!("{count} working strategies found")).green().bold()
    )
}

/// Summary: no working strategies found — red
pub fn summary_no_strategies(protocol: &str) -> String {
    format!(
        "  {}{}: {}",
        CROSS,
        style(protocol).red(),
        style("no working strategies found").red()
    )
}

/// Summary: working without bypass — green
pub fn summary_available(protocol: &str) -> String {
    format!(
        "  {}{}: {}",
        CHECKMARK,
        style(protocol).green(),
        style("working without bypass").green()
    )
}

/// Strategy line: `    → nfqws2 args` in cyan
pub fn strategy_line(args: &str) -> String {
    format!("    {}nfqws2 {}", ARROW, style(args).cyan())
}

/// Stats line: `completed: N | success: N | ...`
pub fn stats_line(
    completed: usize,
    successes: usize,
    failures: usize,
    errors: usize,
    elapsed_secs: f64,
    throughput: f64,
) -> String {
    format!(
        "  completed: {} | success: {} | failed: {} | errors: {} | {:.1}s ({:.1} strat/sec)",
        completed,
        style(successes).green(),
        failures,
        if errors > 0 {
            style(errors).red().to_string()
        } else {
            errors.to_string()
        },
        elapsed_secs,
        throughput,
    )
}

/// Layout manager for scan output. Ensures all text goes through `MultiProgress`
/// so progress bars and vanilla output never interleave.
pub struct ScanScreen {
    multi: MultiProgress,
    divider_bar: Option<ProgressBar>,
    pb: Option<ProgressBar>,
    info_bar: Option<ProgressBar>,
}

impl ScanScreen {
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
            divider_bar: None,
            pb: None,
            info_bar: None,
        }
    }

    /// Print a line above the progress bar (or just to stdout if no bar active).
    pub fn println(&self, msg: &str) {
        let _ = self.multi.println(msg);
    }

    /// Print an empty line.
    pub fn newline(&self) {
        let _ = self.multi.println("");
    }

    /// Set a fixed info line (e.g. ISP info) that stays below the progress bar.
    pub fn set_info(&mut self, msg: &str) {
        let bar = self.multi.add(ProgressBar::new(0));
        bar.set_style(ProgressStyle::with_template("{msg}").unwrap());
        bar.set_message(format!("{}", style(msg).dim()));
        bar.tick();
        self.info_bar = Some(bar);
    }

    /// Clear and remove the info bar.
    pub fn finish_info(&mut self) {
        if let Some(bar) = self.info_bar.take() {
            bar.finish_and_clear();
        }
    }

    /// Create divider + progress bar and add both to `MultiProgress`.
    /// If an info_bar exists, inserts divider and pb before it so info stays at the bottom.
    pub fn begin_progress(&mut self, total: u64) {
        let width = Term::stdout().size().1 as usize;

        let divider = ProgressBar::new(0);
        let pb = ProgressBar::new(total);

        // Add to MultiProgress first, then configure — indicatif needs the draw
        // target set up before set_message/enable_steady_tick take effect.
        let (divider, pb) = if let Some(ref info) = self.info_bar {
            (
                self.multi.insert_before(info, divider),
                self.multi.insert_before(info, pb),
            )
        } else {
            (self.multi.add(divider), self.multi.add(pb))
        };

        divider.set_style(ProgressStyle::with_template("{msg}").unwrap());
        divider.set_message(format!("{}", style("─".repeat(width)).dim()));

        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({per_sec}, ETA {eta})"
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        self.divider_bar = Some(divider);
        self.pb = Some(pb);
    }

    /// Finish and clear both the progress bar and the divider.
    /// The info_bar remains visible.
    pub fn finish_progress(&mut self) {
        if let Some(pb) = self.pb.take() {
            pb.finish_and_clear();
        }
        if let Some(div) = self.divider_bar.take() {
            div.finish_and_clear();
        }
    }

    /// Access the underlying `MultiProgress` (for `run_parallel`).
    pub fn multi(&self) -> &MultiProgress {
        &self.multi
    }

    /// Access the progress bar (for `run_parallel`). Panics if not started.
    pub fn pb(&self) -> &ProgressBar {
        self.pb.as_ref().expect("progress bar not started")
    }
}
