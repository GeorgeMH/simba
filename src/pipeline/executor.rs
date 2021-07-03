use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use linked_hash_map::LinkedHashMap;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::sync::RwLock;

use crate::config::{HttpMethod, PipelineDef, PipelineStepDef, RequestBody};
use crate::error::SimbaError;
use crate::lua::LuaContext;
use crate::output::PipelineEventHandler;
use crate::pipeline::{
    ExecutionResult, HttpResponse, Pipeline, StepResult, StepTask, TaskState,
};
use crate::Result;

#[derive(Clone)]
pub struct PipelineExecutor {
    output_writer: Box<dyn PipelineEventHandler>,
    lua: Arc<RwLock<LuaContext>>,
    client: reqwest::Client,
}

impl PipelineExecutor {
    pub async fn new(
        pipeline: &PipelineDef,
        output_writer: Box<dyn PipelineEventHandler>,
    ) -> Result<PipelineExecutor> {
        let lua = LuaContext::new()?;
        lua.execute_named_templates(&pipeline.globals).await?;

        Ok(PipelineExecutor {
            output_writer,
            lua: Arc::new(RwLock::new(lua)),
            client: reqwest::Client::new(),
        })
    }

    pub fn build_pipeline(
        pipeline_def: &PipelineDef,
        // _stages_to_exec: Vec<String>,
    ) -> Result<Pipeline> {
        let mut pipeline = Pipeline::new(pipeline_def.name.clone());

        for step in &pipeline_def.steps {
            let stage = pipeline.get_stage(step.stage.as_str());
            stage.add_task(StepTask::new(step.clone(), stage.id()));
        }

        Ok(pipeline)
    }

    pub async fn execute_pipeline(
        &self,
        pipeline: &PipelineDef,
        // stages_to_exec: Vec<String>,
    ) -> Result<()> {
        let pipeline = Self::build_pipeline(pipeline)?;

        self.output_writer.pipeline_init(&pipeline);
        for stage in pipeline.stages() {
            self.output_writer.stage_start(stage);

            let local_set = tokio::task::LocalSet::new();

            for task in stage.tasks() {
                let child_self = self.clone();
                let child_task = task.clone(); // TODO: Can we eliminate this clone
                log::info!("Spawn Local Task {}", task.id);
                let _foo = local_set.spawn_local(async move {
                    let task_id = child_task.id;
                    log::info!("Child Step: {}", task_id);
                    child_self.execute_step(child_task).await;
                    log::info!("Finish Child Step: {}", task_id);
                });
            }

            log::info!("Await stage");
            local_set.await;
            log::info!("Call Stage End");
            self.output_writer.stage_end(stage);
        }

        self.output_writer.pipeline_finish(&pipeline);

        Ok(())
    }

    pub async fn execute_step(&self, step_task: StepTask) {
        log::info!("Pipeline Step: {}", &step_task.step.desc);
        let mut step_task = step_task;
        self.output_writer
            .task_update(&step_task, TaskState::Executing("Starting".to_string()));
        step_task.start_time = Instant::now();

        match self.render_api_step(&step_task).await {
            // failed rendering the final step
            Err(error) => {
                return self
                    .handle_error(&step_task, &error, "Failed rendering step")
                    .await;
            }
            Ok(rendered_step) => {
                step_task.rendered_step = Some(rendered_step);

                // Test the `when` clause if it is set
                if !self.execute_when_clause(&step_task).await {
                    return;
                }

                match self.make_http_call(&step_task).await {
                    Err(error) => {
                        self.handle_error(&step_task, &error, "HTTP Error")
                            .await
                    }
                    Ok(response) => self.execute_post_script(&step_task, response).await,
                }
            }
        }
    }

    async fn handle_error(&self, step_task: &StepTask, simba_error: &SimbaError, message: &str) {
        let msg = format!("{}: {}", message, simba_error);

        let result = StepResult {
            step: step_task.step.clone(),
            rendered_step: step_task
                .rendered_step
                .as_ref()
                .map(std::clone::Clone::clone),
            result: ExecutionResult::Error(msg),
            execution_time_ms: step_task.start_time.elapsed().as_millis(),
        };

        self.output_writer
            .task_update(step_task, TaskState::Result(result));
    }

    async fn execute_post_script(&self, step_task: &StepTask, response: HttpResponse) {
        self.output_writer
            .task_update(step_task, TaskState::Executing("Post Script".to_string()));
        let lua = self.lua.read().await;
        if let Err(error) = lua.set_on_context("http_response", &response).await {
            return self
                .handle_error(step_task, &error, "Failed updating context http_response")
                .await;
        }

        let rendered_step = step_task
            .rendered_step
            .as_ref()
            .unwrap_or_else(|| panic!("Expected Rendered Step: {}", step_task.id));

        let lua = self.lua.read().await;

        let post_script_result = if let Some(post_script) = rendered_step.post_script.as_ref() {
            match lua.evaluate_bool(post_script).await {
                Ok(post_script_result) => Some(post_script_result),
                Err(error) => {
                    self.handle_error(step_task, &error, "Post Script Error")
                        .await;
                    return;
                }
            }
        } else {
            None
        };

        let mut response = response;
        response.post_script_result = post_script_result;

        let step_result = StepResult {
            step: step_task.step.clone(),
            rendered_step: step_task.rendered_step.clone(),
            execution_time_ms: step_task.start_time.elapsed().as_millis(),
            result: ExecutionResult::Response(response),
        };

        self.output_writer
            .task_update(step_task, TaskState::Result(step_result));
    }

    /// Execute when clause if provided
    async fn execute_when_clause(&self, step_task: &StepTask) -> bool {
        self.output_writer.task_update(
            step_task,
            TaskState::Executing("Evaluating When".to_string()),
        );
        let rendered_step = step_task
            .rendered_step
            .as_ref()
            .unwrap_or_else(|| panic!("Expected Rendered Step: {}", step_task.id));

        if let Some(when_clause) = rendered_step.when.as_ref() {
            let lua = self.lua.read().await;
            return match lua.evaluate_bool(when_clause).await {
                Err(error) => {
                    self.handle_error(step_task, &error, "Failed executing when clause")
                        .await;
                    false
                }
                Ok(result) => {
                    if !result {
                        let skip_message = format!(
                            "Skipped: When clause evaluated false: {}",
                            when_clause.replace("\n", "\\n")
                        );
                        let step_result = StepResult {
                            step: step_task.step.clone(),
                            rendered_step: step_task.rendered_step.clone(),
                            execution_time_ms: step_task.start_time.elapsed().as_millis(),
                            result: ExecutionResult::Skipped(skip_message),
                        };
                        self.output_writer
                            .task_update(step_task, TaskState::Result(step_result))
                    }

                    result
                }
            };
        }

        true
    }

    async fn make_http_call(&self, step_task: &StepTask) -> Result<HttpResponse> {
        let rendered_step = step_task.rendered_step.as_ref().unwrap(); // TODO

        let start_time = Instant::now();
        let url = rendered_step.url.clone();
        let mut request = match rendered_step.method {
            HttpMethod::Post => self.client.post(url.as_str()),
            HttpMethod::Get => self.client.get(url.as_str()),
            HttpMethod::Put => self.client.put(url.as_str()),
            HttpMethod::Delete => self.client.delete(url.as_str()),
            // TODO: Others
        };

        if let Some(timeout_ms) = rendered_step.timeout_ms {
            request = request.timeout(Duration::from_millis(timeout_ms));
        }

        if let Some(headers) = rendered_step.headers.as_ref() {
            for (header, value) in headers {
                request = request.header(
                    HeaderName::from_bytes(header.as_bytes())?,
                    HeaderValue::from_bytes(value.as_bytes())?,
                );
            }
        }

        if let Some(body) = rendered_step.body.as_ref() {
            match body {
                RequestBody::BodyString(body_string) => {
                    request = request.body(body_string.clone());
                }
                unhandled_body => {
                    return Err(SimbaError::Other(format!(
                        "Unexpected RequestBody type {:?}",
                        unhandled_body
                    )));
                }
            }
        }

        log::info!("Calling {}", url);
        self.output_writer
            .task_update(step_task, TaskState::Executing(format!("Calling: {}", url)));
        let result = request.send().await?;

        let status = result.status().into();
        let headers = convert_header_map(result.headers());

        let body_raw = result.bytes().await?.to_vec();
        let body_string = String::from_utf8(body_raw)?;

        let duration = start_time.elapsed();

        Ok(HttpResponse {
            post_script_result: None,
            status,
            headers,
            body_string: Some(body_string),
            execution_time_ms: duration.as_millis(),
        })
    }

    async fn render_api_step(&self, step_task: &StepTask) -> Result<PipelineStepDef> {
        self.output_writer
            .task_update(step_task, TaskState::Executing("Rendering".to_string()));

        let lua = self.lua.read().await;
        let url = lua.execute_template(&step_task.step.url).await?;
        // let timeout_ms = step_task.rendered_step step.timeout_ms;
        let timeout_ms = step_task.step.timeout_ms;

        let headers = match step_task.step.headers.as_ref() {
            Some(headers) => {
                let mut generated_headers = LinkedHashMap::new();
                for (header, value) in headers {
                    let header = lua.execute_template(header).await?;
                    let value = lua.execute_template(value).await?;
                    generated_headers.insert(header.clone(), value.clone());
                }
                Some(generated_headers)
            }
            None => None,
        };

        let body = match &step_task.step.body {
            Some(request_body) => {
                let rendered_body = match request_body {
                    RequestBody::BodyString(body) => body.clone(),
                    RequestBody::BodyStringTemplate(template) => lua.execute_template(template).await?,
                };
                Some(RequestBody::BodyString(rendered_body))
            }
            None => None,
        };

        Ok(PipelineStepDef {
            desc: step_task.step.desc.clone(),
            method: step_task.step.method.clone(),
            stage: step_task.step.stage.clone(),
            url,
            headers,
            timeout_ms,
            when: step_task.step.when.clone(),
            body,
            tags: step_task.step.tags.clone(),
            post_script: step_task.step.post_script.clone(),
        })
    }
}

fn convert_header_map(header_map: &HeaderMap) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    for (key, value) in header_map.iter() {
        let k = key.as_str().to_owned();
        let v = String::from_utf8_lossy(value.as_bytes()).into_owned();
        headers.insert(k, v);
    }
    headers
}
