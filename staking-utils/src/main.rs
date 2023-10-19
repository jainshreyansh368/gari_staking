mod params;

use aws_sdk_ssm::Error;
use clap::{Parser, Subcommand};
use params::FetchParams;

use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

// A fictional versioning CLI
#[derive(Debug, Parser)] // requires `derive` feature
#[clap(name = "staking-utils")]
#[clap(about = "CLI Utility", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    commands: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    FetchParams(FetchParams),
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "warn");
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive(
                "staking_utils=info"
                    .parse()
                    .expect("Error parsing directive"),
            ),
        )
        .with_span_events(FmtSpan::FULL)
        .init();

    let cli: Cli = Cli::parse();

    match cli.commands {
        Commands::FetchParams(fetch_params) => {
            let client = params::get_client(fetch_params.aws_region.clone()).await;
            params::write_file(&client, fetch_params).await;
        }
    }

    Ok(())
}
