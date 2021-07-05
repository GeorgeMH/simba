use crate::script::ScriptContext;
use crate::Result;
use handlebars::Handlebars;

pub trait TemplateEngine: Clone + Send + Sync {
    fn render(&self, template: &str, context: &ScriptContext) -> Result<String>;
}

#[derive(Clone)]
pub struct HandlebarsEngine<'reg> {
    template: Handlebars<'reg>,
}

impl<'reg> HandlebarsEngine<'reg> {
    pub fn new() -> Self {
        Self {
            template: Handlebars::new(),
        }
    }
}

impl<'reg> TemplateEngine for HandlebarsEngine<'reg> {
    fn render(&self, template: &str, context: &ScriptContext) -> Result<String> {
        let rendered_string = self.template.render_template(template, context.data())?;
        log::info!("Rendered {} as {}", template, rendered_string);
        Ok(rendered_string)
    }
}
