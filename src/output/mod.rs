/// This crate provides an interface used to emit events as a pipeline is executed.
use dyn_clone::DynClone;
use async_trait::async_trait;

use crate::pipeline::{Pipeline, Stage, StepResult, StepTask};

pub use crate::output::console::ConsoleWriter;
pub use crate::output::json::JsonWriter;

mod console;
mod json;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum TaskUpdate {
    Processing(String),
    Finished(StepResult),
}

#[async_trait]
pub trait PipelineEventHandler: DynClone + Send + Sync {

    async fn pipeline_init(&self, pipeline: &Pipeline);

    async fn stage_start(&self, stage: &Stage);

    async fn task_update(&self, task: &StepTask, task_state: TaskUpdate);

    async fn stage_end(&self, stage: &Stage);

    async fn pipeline_finish(&self, pipeline: &Pipeline);
}

dyn_clone::clone_trait_object!(PipelineEventHandler);
