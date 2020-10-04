use anyhow::{Context, Result};
use serde::ser::Serialize;
use tera::{Context as TeraContext, Tera};

#[derive(Clone, Debug)]
pub struct Templates {
    tera: std::sync::Arc<Tera>,
    ctx: TeraContext,
}

impl Templates {
    pub fn new(tera: std::sync::Arc<Tera>, base_ctx: TeraContext) -> Self {
        Self { tera: tera, ctx: base_ctx }
    }
    pub fn add_context<T: Serialize + ?Sized>(&self, key: &str, val: &T) -> Templates {
        let mut new_ctx = self.ctx.clone();
        new_ctx.insert(key, val);

        Templates { tera: self.tera.clone(), ctx: new_ctx }
    }

    pub fn render(&self, template: &str) -> Result<String> {
        self.tera.render(template, &self.ctx)
            .context("tera template rendering failed")
    }
}
