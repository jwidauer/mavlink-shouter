use anyhow::Result;
use clap::Parser;
use std::path;

use mavlink_shouter::{config, MAVLinkShouter};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The configuration file to use.
    #[arg(short, long, default_value = "config/default.yml")]
    config: path::PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // console_subscriber::init();
    env_logger::builder()
        .format_module_path(false)
        .format_target(true)
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    // let guard = pprof::ProfilerGuardBuilder::default()
    //     .frequency(10000)
    //     .build()?;

    let args = Args::parse();
    let settings = config::Settings::load(&args.config)?;

    MAVLinkShouter::new(settings)?.run();

    tokio::signal::ctrl_c().await?;

    // if let Ok(report) = guard.report().build() {
    //     let file = std::fs::File::create("flamegraph.svg")?;
    //     report.flamegraph(file)?;
    // }

    Ok(())
}
