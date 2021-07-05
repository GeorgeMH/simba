use crate::script::ScriptContext;
use crate::Result;
use handlebars::Handlebars;

pub trait TemplateEngine: Clone + Send + Sync {
    fn template(&self, template: &str, context: &ScriptContext) -> Result<String>;
}

#[derive(Clone)]
pub struct HandlebarsEngine {}

impl HandlebarsEngine {
    pub fn new() -> Self {
        Self {}
    }
}

impl TemplateEngine for HandlebarsEngine {
    fn template(&self, template: &str, context: &ScriptContext) -> Result<String> {
        let handlebars = Handlebars::new();
        let rendered_string = handlebars.render_template(template, context.data())?;
        log::info!("Rendered {} as {}", template, rendered_string);
        Ok(rendered_string)
    }
}
