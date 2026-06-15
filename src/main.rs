mod scanner;
mod analyzer;
mod generator;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gitmind")]
#[command(version = "0.1.0")]
#[command(about = "Auto-generate AI context docs for any codebase")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan current repo and generate knowledge docs
    Sync {
        /// Output directory for generated files
        #[arg(short, long, default_value = ".gitmind")]
        output: String,

        /// Languages to scan (comma-separated, default: all supported)
        #[arg(short, long)]
        lang: Option<String>,
    },

    /// Show scan stats without writing files
    Stats,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sync { output, lang } => {
            println!("🔍 Scanning project...\n");

            let files = scanner::scan(".", lang.as_deref())?;
            println!("📁 Found {} source files", files.len());

            let analysis = analyzer::analyze(&files)?;
            println!("🧩 Identified {} modules", analysis.modules.len());
            println!("📦 Found {} public interfaces", analysis.interfaces.len());

            generator::generate(&output, &analysis)?;
            println!("\n✅ Generated docs in {}/", output);
            println!("   - AGENTS.md (AI context)");
            println!("   - architecture.md (module structure)");
            println!("   - knowledge.md (full project docs)");
        }
        Commands::Stats => {
            let files = scanner::scan(".", None)?;
            println!("Project Stats:");
            println!("  Files: {}", files.len());
            for f in &files {
                let path = f.path.to_string_lossy().replace('\\', "/");
                println!("    {} ({} lines)", path, f.line_count);
            }
        }
    }

    Ok(())
}
