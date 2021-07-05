#![deny(clippy::pedantic)]
#![deny(clippy::all)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::non_ascii_literal)]

use clap::{AppSettings, Clap};
use log::LevelFilter;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;

use pipeline::PipelineExecutor;

use crate::config::load_pipeline_def;
pub use crate::error::SimbaResult as Result;
use crate::event::{ConsoleEventHandler, JsonEventHandler, PipelineEventHandlerHolder};
use crate::script::lua::LuaScriptEngine;
use crate::template::HandlebarsEngine;

mod config;
mod error;
mod event;
mod pipeline;
mod script;
mod template;

/// simba is a CLI based HTTP scripting engine
#[derive(Clap, Debug)]
#[clap(version = "0.1.0", author = "George Haney <george@georgemh.com>")]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    /// Stages execute from the pipeline
    #[clap(short, long)]
    stages: Vec<String>,

    /// Path to the pipeline file
    #[clap(short, long)]
    pipeline: String,

    /// Output the response body
    #[clap(short, long)]
    output_response_body: bool,

    /// Output results as a JSON stream
    #[clap(short, long)]
    json_output: bool,

    /// Path to event logs to
    #[clap(short, long)]
    log_file_path: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts: Opts = Opts::parse();
    configure_logging(&opts)?;

    let pipeline = load_pipeline_def(&opts.pipeline)?;

    let event_handler = create_event_handler(&opts);
    let script_engine = LuaScriptEngine::new()?;
    let template_engine = HandlebarsEngine::new();

    let executor = PipelineExecutor::new(event_handler, script_engine, template_engine).await?;
    executor
        .execute_pipeline(&pipeline, &opts.stages)
        .await?;

    Ok(())
}

fn configure_logging(opts: &Opts) -> Result<()> {
    if let Some(log_file) = opts.log_file_path.as_ref() {
        let file_appender = FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{d} - {m}{n}")))
            .build(log_file)?;

        let config = Config::builder()
            .appender(Appender::builder().build("file", Box::new(file_appender)))
            .build(Root::builder().appender("file").build(LevelFilter::Debug))?;

        let _handle = log4rs::init_config(config)?;
    }

    Ok(())
}

fn create_event_handler(opts: &Opts) -> PipelineEventHandlerHolder {
    if opts.json_output {
        PipelineEventHandlerHolder::Json(JsonEventHandler::new())
    } else {
        PipelineEventHandlerHolder::Console(ConsoleEventHandler::new())
    }
}
