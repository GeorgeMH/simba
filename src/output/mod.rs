/// This crate provides an interface used to emit events as a pipeline is executed.

use dyn_clone::DynClone;

use crate::pipeline::{Pipeline, Stage, StepTask, TaskState};

pub use crate::output::console::ConsoleWriter;
pub use crate::output::json::JsonWriter;

mod console;
mod json;

pub trait PipelineEventHandler: DynClone + Send + Sync {
    fn pipeline_init(&self, pipeline: &Pipeline);

    fn stage_start(&self, stage: &Stage);

    fn task_update(&self, task: &StepTask, task_state: TaskState);

    fn stage_end(&self, stage: &Stage);

    fn pipeline_finish(&self, pipeline: &Pipeline);
}

dyn_clone::clone_trait_object!(PipelineEventHandler);
