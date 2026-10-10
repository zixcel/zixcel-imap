use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "imap-curator-collector",
    version,
    about = "Collect From domains from IMAP INBOX into DB"
)]
pub struct Config {
    #[arg(long)]
    pub log_filter: Option<String>,
    #[arg(long)]
    pub account_id: Option<String>,
    #[arg(long)]
    pub max_batches: Option<u32>,
    #[arg(long)]
    pub batch_size: Option<u32>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, value_enum, default_value_t = Mode::Daemon)]
    pub mode: Mode,
    #[arg(long, default_value_t = 21600)]
    pub interval_secs: u64,
    #[arg(long)]
    pub database_url: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Mode {
    Once,
    Daemon,
}

impl Config {
    pub fn load() -> Self {
        Config::parse()
    }
}
