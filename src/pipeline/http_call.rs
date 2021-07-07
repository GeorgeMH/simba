use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::config::{HttpMethod, RequestBody};
use crate::error::{SimbaError, SimbaResult};
use crate::pipeline::PipelineStep;
use crate::pipeline::{HttpResponse, StepTask, TaskState};
use crate::script::ScriptContext;

pub struct HttpCallPipelineStep {
    client: reqwest::Client,
}

impl HttpCallPipelineStep {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Display for HttpCallPipelineStep {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP Call")
    }
}

#[async_trait]
impl PipelineStep for HttpCallPipelineStep {
    #[allow(clippy::cast_possible_truncation)]
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> SimbaResult<TaskState> {
        log::info!("HTTP Call: {:?}", script_context);
        let rendered_step = step_task
            .rendered_step
            .as_ref()
            .unwrap_or_else(|| panic!("Expected Rendered Step: {}", step_task.id));

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

        log::info!("Calling URL: {}", url);
        let result = request.send().await?;

        let status = result.status().into();
        let headers = convert_header_map(result.headers());

        let body_raw = result.bytes().await?.to_vec();
        // TODO: Support binary / non-utf8 responses
        let body_string = String::from_utf8(body_raw)?;

        let duration = start_time.elapsed();

        script_context.set("call_duration_ms", duration.as_millis() as u64)?;
        script_context.set(
            "http_response",
            HttpResponse {
                post_script_result: None,
                status,
                headers,
                body_string: Some(body_string),
                execution_time_ms: duration.as_millis() as u64,
            },
        )?;

        log::info!("Http Call Finished");

        Ok(TaskState::Complete("HTTP Call finished".to_string()))
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
