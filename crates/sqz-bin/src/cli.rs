use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "sqz", version, about = "Concutter - prompt compression proxy")]
pub struct Cli {
    /// Config file path
    #[arg(short, long, default_value = "concutter.toml")]
    pub config: String,

    /// Server host
    #[arg(long)]
    pub host: Option<String>,

    /// Server port
    #[arg(long)]
    pub port: Option<u16>,

    /// Database path
    #[arg(long)]
    pub db: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long)]
    pub log_level: Option<String>,
}
