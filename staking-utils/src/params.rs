use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ssm::{config::Region, Client, Error};
use clap::Args;
use serde::Deserialize;
use std::fs;
use std::fs::File;
use std::io::Write;
use tracing::{debug, error};

#[derive(Debug, Args)]
#[clap(args_conflicts_with_subcommands = true)]
#[clap(version = "v0.1.0", about = "Fetch secrets from AWS secrets manager")]
pub struct FetchParams {
    /// Path of the file with new line separated values with list of params to fetch from AWS secrets manager
    #[clap(short, long)]
    pub input_file: String,

    /// AWS Region
    #[clap(short = 'r', long)]
    pub aws_region: Option<String>,

    /// AWS Secrets manager path
    #[clap(short = 'p', long)]
    pub aws_path: String,

    /// Output toml file name which will contain values for params provided.
    #[clap(short, long)]
    pub output_file: String,

    /// Optional tag to be added in output_file, viz. [default]
    #[clap(short, long)]
    pub tag: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct AppConfig {
    aws_access_key_id: String,
    aws_secret_access_key: String,
}

pub async fn get_client(aws_region: Option<String>) -> Client {
    let region_provider = RegionProviderChain::first_try(aws_region.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-east-1"));
    let shared_config = aws_config::from_env().region(region_provider).load().await;
    Client::new(&shared_config)
}

pub async fn write_file(client: &Client, params: FetchParams) {
    let contents =
        fs::read_to_string(params.input_file).expect("Something went wrong reading the flie");
    let mut lines = contents.lines();
    let mut out_file = File::create(params.output_file).unwrap();
    match params.tag {
        Some(t) => match writeln!(out_file, "{}", t) {
            Ok(v) => v,
            Err(_) => error!("Failed to write to file"),
        },
        None => {}
    }

    while let Some(line) = lines.next() {
        let arr: Vec<&str> = line.split(",").collect();
        let name = arr[0];
        debug!("name: {}", params.aws_path.to_owned() + name);
        let secret = get_secrets(client, params.aws_path.to_owned(), name).await;
        match secret {
            Ok(val) => {
                let buff = if arr[1].eq("num") || arr[1].eq("bool") {
                    writeln!(out_file, "{} = {}", name, val)
                } else {
                    writeln!(out_file, "{} = \"{}\"", name, val)
                };
                match buff {
                    Ok(v) => v,
                    Err(_) => error!("Failed to write to file"),
                }
            }
            Err(err) => error!("Failed to fetch secrets from aws for {:?}: {:?}", name, err),
        };
    }
}

async fn get_secrets(client: &Client, path: String, name: &str) -> Result<String, Error> {
    let resp = client
        .get_parameter()
        .name(path + name)
        .with_decryption(true)
        .send()
        .await?;

    Ok(resp
        .parameter()
        .unwrap()
        .value
        .as_ref()
        .unwrap()
        .to_string())
}
