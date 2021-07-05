use std::fmt::{Display, Formatter};

use async_trait::async_trait;

use crate::error::SimbaResult;
use crate::pipeline::PipelineStep;
use crate::pipeline::{StepTask, TaskState};
use crate::script::{ScriptContext, ScriptEngine};

pub struct PostScriptPipelineStep<S: ScriptEngine> {
    script: S,
}

impl<S: ScriptEngine> PostScriptPipelineStep<S> {
    pub fn new(script: S) -> Self {
        Self { script }
    }
}

impl<S: ScriptEngine> Display for PostScriptPipelineStep<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Post Script")
    }
}

#[async_trait]
impl<S: ScriptEngine> PipelineStep for PostScriptPipelineStep<S> {
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> SimbaResult<TaskState> {
        let rendered_step = step_task
            .rendered_step
            .as_ref()
            .unwrap_or_else(|| panic!("Expected Rendered Step: {}", step_task.id));

        if let Some(post_script) = rendered_step.post_script.as_ref() {
            log::info!("Calling PostScript\n{:#?}", script_context);
            let (updated_context, post_script_result): (ScriptContext, bool) = self
                .script
                .execute(script_context.clone(), post_script)
                .await?
                .result()?;

            script_context.merge(updated_context)?;
            log::info!("Finished PostScript\n{:#?}", script_context);

            let mut http_response = script_context.get_http_response()?;
            http_response.post_script_result = Some(post_script_result);
            script_context.set_http_response(http_response);

            if !post_script_result {
                return Ok(TaskState::Error("Post Script returned false".to_string()));
            }
        }

        Ok(TaskState::Complete("Post Script Finished".to_string()))
    }
}
