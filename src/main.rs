mod config;
mod constants;
mod utils;

use actix_web::http::StatusCode;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use actix_web_httpauth::middleware::HttpAuthentication;
use bytes::Bytes;
use clap::Parser;
use colored::Colorize;
use config::{log_config_information, read_config, Config};
use constants::{VIEWER_ENDING_BYTES, VIEWER_TEMPLATE_BYTES};
use futures::stream;
use futures::{StreamExt, TryStreamExt};
use std::io::Error;
use std::process::Stdio;
use tokio::process::Command;
use tokio_util::io::ReaderStream;
use utils::{check_executables_root, request_validator, transform_bytes, IntoHttpError};

pub struct BarnState {
    pub config: Config,
}

#[get("")]
async fn root_handler(
    path: web::Path<String>,
    data: web::Data<BarnState>,
) -> Result<HttpResponse, actix_web::Error> {
    let options = &data.config.options;
    let path = path.to_string();
    let program_path = options.root.join(&path);

    let cmd = Command::new(&program_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .templated_error(
            &format!("Unable to spawn executable '{}'", path).to_string(),
            StatusCode::INTERNAL_SERVER_ERROR,
        )?;

    let stdout = cmd.stdout.generic_error()?;
    let stderr = cmd.stderr.generic_error()?;

    let stdout_stream = ReaderStream::new(stdout).map_ok(|bytes| transform_bytes(bytes, "stdout"));
    let stderr_stream = ReaderStream::new(stderr).map_ok(|bytes| transform_bytes(bytes, "stderr"));
    let merged_stream = futures::stream::select(stdout_stream, stderr_stream);

    let start_stream = stream::once(async { Ok::<Bytes, Error>(VIEWER_TEMPLATE_BYTES.clone()) });
    let end_stream = stream::once(async { Ok::<Bytes, Error>(VIEWER_ENDING_BYTES.clone()) });

    let final_stream = start_stream.chain(merged_stream).chain(end_stream);

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .append_header(("Transfer-Encoding", "chunked"))
        .streaming(final_stream))
}

async fn default_handler(path: web::Path<String>) -> impl Responder {
    HttpResponse::build(StatusCode::NOT_FOUND)
        .content_type("text/html; charset=utf-8")
        .body(path.to_string())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of config file
    #[arg(short, long)]
    config: Option<String>,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let (config, config_path) = read_config(args.config)?;
    let options = &config.options;

    check_executables_root(&options.root)?;
    log_config_information(&config, &options.root)?;

    let barn_state = web::Data::new(BarnState {
        config: config.clone(),
    });

    println!("\n{} {}", "Config path:".blue().bold(), config_path);
    println!(
        "{} {}{}{}",
        "Running on:".blue().bold(),
        options.host,
        ":".bold(),
        options.port
    );
    println!(
        "{} {}",
        "Executables' root:".blue().bold(),
        options
            .root
            .canonicalize()
            .unwrap_or_else(|_| options.root.clone())
            .display()
    );

    HttpServer::new(move || {
        let auth_middleware = HttpAuthentication::basic(request_validator);

        App::new()
            .app_data(barn_state.clone())
            .service(
                web::scope("/{path_string}")
                    .wrap(auth_middleware)
                    .service(root_handler),
            )
            .default_service(web::route().to(default_handler))
    })
    .bind((options.host.clone(), options.port))?
    .run()
    .await?;

    println!("Exiting...");
    Ok(())
}
