use crate::pipeline::PipelineStep;
use crate::pipeline::{StepTask, TaskState};
use crate::script::ScriptContext;
use crate::template::TemplateEngine;
use std::fmt::{Display, Formatter};

use crate::config::{PipelineStepDef, RequestBody};
use crate::Result;
use linked_hash_map::LinkedHashMap;

use async_trait::async_trait;

pub struct RenderPipelineStep<T: TemplateEngine> {
    template: T,
}

impl<T: TemplateEngine> RenderPipelineStep<T> {
    pub fn new(template: T) -> Self {
        Self { template }
    }
}

impl<T: TemplateEngine> Display for RenderPipelineStep<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rendering Step")
    }
}

#[async_trait]
impl<T: TemplateEngine> PipelineStep for RenderPipelineStep<T> {
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> Result<TaskState> {
        let url = self.template.render(&step_task.step.url, &script_context)?;
        let timeout_ms = step_task.step.timeout_ms;

        let headers = match step_task.step.headers.as_ref() {
            Some(headers) => {
                let mut generated_headers = LinkedHashMap::new();
                for (header, value) in headers {
                    let header = self.template.render(header, &script_context)?;
                    let value = self.template.render(value, &script_context)?;
                    generated_headers.insert(header, value);
                }
                Some(generated_headers)
            }
            None => None,
        };
        let body = match &step_task.step.body {
            Some(request_body) => {
                let rendered_body = match request_body {
                    RequestBody::BodyString(body) => body.clone(),
                    RequestBody::BodyStringTemplate(template) => {
                        self.template.render(template, &script_context)?
                    }
                };
                Some(RequestBody::BodyString(rendered_body))
            }
            None => None,
        };

        let rendered_step = PipelineStepDef {
            desc: step_task.step.desc.clone(),
            method: step_task.step.method.clone(),
            stage: step_task.step.stage.clone(),
            url,
            headers,
            timeout_ms,
            when: step_task.step.when.clone(),
            body,
            post_script: step_task.step.post_script.clone(),
        };

        step_task.rendered_step = Some(rendered_step);
        Ok(TaskState::Complete("Rendered".to_string()))
    }
}
