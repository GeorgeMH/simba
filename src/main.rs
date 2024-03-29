#![deny(clippy::pedantic)]
#![deny(clippy::all)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::non_ascii_literal)]

use clap::{Parser};
use log::LevelFilter;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;

use pipeline::PipelineExecutor;

use crate::config::load_pipeline_def;
pub use crate::error::SimbaResult as Result;
use crate::event::console::PrintOptions;
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
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Opts {
    /// Stages to execute from the pipeline
    #[arg(short, long)]
    stages: Vec<String>,

    /// Path to the pipeline file
    #[arg(short, long)]
    pipeline: String,

    /// Output results as a JSON stream
    #[arg(short, long)]
    json_output: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Path to write logs to
    #[arg(short, long)]
    log_file_path: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_panic_hook();

    let opts: Opts = Opts::parse();
    configure_logging(&opts)?;

    let pipeline_def = load_pipeline_def(&opts.pipeline).await?;

    let event_handler = create_event_handler(&opts);
    let script_engine = LuaScriptEngine::new()?;
    let template_engine = HandlebarsEngine::new();

    let executor = PipelineExecutor::new(event_handler, script_engine, template_engine).await?;
    executor.execute_pipeline(&pipeline_def, &opts.stages).await?;

    Ok(())
}

fn setup_panic_hook() {
    let original_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        original_panic_hook(panic_info);
        std::process::exit(1);
    }));
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
        // TODO: Configure each print option individually
        let print_options = PrintOptions {
            print_execution_tree: opts.verbose,
            print_request_headers: opts.verbose,
            print_response_headers: opts.verbose,
            print_body: opts.verbose,
        };
        PipelineEventHandlerHolder::Console(ConsoleEventHandler::new(print_options))
    }
}
