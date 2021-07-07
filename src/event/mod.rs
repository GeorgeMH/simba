use async_trait::async_trait;

use crate::pipeline::{Pipeline, Stage, StepResult, StepTask};

pub use crate::event::console::ConsoleEventHandler;
pub use crate::event::json::JsonEventHandler;

pub mod console;
pub mod json;

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
pub enum PipelineEventHandlerHolder {
    Console(ConsoleEventHandler),
    Json(JsonEventHandler),
}

#[async_trait]
impl PipelineEventHandler for PipelineEventHandlerHolder {
    async fn pipeline_init(&self, pipeline: &Pipeline) {
        match self {
            PipelineEventHandlerHolder::Console(h) => h.pipeline_init(pipeline).await,
            PipelineEventHandlerHolder::Json(h) => h.pipeline_init(pipeline).await,
        }
    }

    async fn stage_start(&self, stage: &Stage) {
        match self {
            PipelineEventHandlerHolder::Console(h) => h.stage_start(stage).await,
            PipelineEventHandlerHolder::Json(h) => h.stage_start(stage).await,
        }
    }

    async fn task_update(&self, task: &StepTask, task_update: TaskUpdate) {
        match self {
            PipelineEventHandlerHolder::Console(h) => h.task_update(task, task_update).await,
            PipelineEventHandlerHolder::Json(h) => h.task_update(task, task_update).await,
        }
    }

    async fn stage_end(&self, stage: &Stage) {
        match self {
            PipelineEventHandlerHolder::Console(h) => h.stage_end(stage).await,
            PipelineEventHandlerHolder::Json(h) => h.stage_end(stage).await,
        }
    }

    async fn pipeline_finish(&self, pipeline: &Pipeline) {
        match self {
            PipelineEventHandlerHolder::Console(h) => h.pipeline_finish(pipeline).await,
            PipelineEventHandlerHolder::Json(h) => h.pipeline_finish(pipeline).await,
        }
    }
}
