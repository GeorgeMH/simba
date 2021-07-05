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
pub trait PipelineEventHandler: Clone + Send + Sync {
    async fn pipeline_init(&self, pipeline: &Pipeline);

    async fn stage_start(&self, stage: &Stage);

    async fn task_update(&self, task: &StepTask, task_state: TaskUpdate);

    async fn stage_end(&self, stage: &Stage);

    async fn pipeline_finish(&self, pipeline: &Pipeline);
}

#[derive(Clone)]
pub enum PipelineEventHandlerEnum {
    Console(ConsoleWriter),
    Json(JsonWriter),
}

#[async_trait]
impl PipelineEventHandler for PipelineEventHandlerEnum {
    async fn pipeline_init(&self, pipeline: &Pipeline) {
        match self {
            PipelineEventHandlerEnum::Console(h) => h.pipeline_init(pipeline).await,
            PipelineEventHandlerEnum::Json(h) => h.pipeline_init(pipeline).await,
        }
    }

    async fn stage_start(&self, stage: &Stage) {
        match self {
            PipelineEventHandlerEnum::Console(h) => h.stage_start(stage).await,
            PipelineEventHandlerEnum::Json(h) => h.stage_start(stage).await,
        }
    }

    async fn task_update(&self, task: &StepTask, task_update: TaskUpdate) {
        match self {
            PipelineEventHandlerEnum::Console(h) => h.task_update(task, task_update).await,
            PipelineEventHandlerEnum::Json(h) => h.task_update(task, task_update).await,
        }
    }

    async fn stage_end(&self, stage: &Stage) {
        match self {
            PipelineEventHandlerEnum::Console(h) => h.stage_end(stage).await,
            PipelineEventHandlerEnum::Json(h) => h.stage_end(stage).await,
        }
    }

    async fn pipeline_finish(&self, pipeline: &Pipeline) {
        match self {
            PipelineEventHandlerEnum::Console(h) => h.pipeline_finish(pipeline).await,
            PipelineEventHandlerEnum::Json(h) => h.pipeline_finish(pipeline).await,
        }
    }
}
