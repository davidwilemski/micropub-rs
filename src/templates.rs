use std::sync::Arc;

use anyhow::{Context, Result};
use indoc::indoc;
use serde::ser::Serialize;
use tera::{Context as TeraContext, Tera};

#[derive(Clone, Debug)]
pub struct Templates {
    tera: std::sync::Arc<Tera>,
    ctx: TeraContext,
}

impl Templates {
    pub fn new(tera: std::sync::Arc<Tera>, base_ctx: TeraContext) -> Self {
        Self {
            tera: tera,
            ctx: base_ctx,
        }
    }

    pub fn atom_default(base_ctx: TeraContext) -> Self {
        let atom_template = indoc! {r#"
        <?xml version="1.0" encoding="utf-8"?>

        <feed xmlns="http://www.w3.org/2005/Atom">
        <title>David's Blog</title>
        <link href="https://davidwilemski.com/" rel="alternate"/>
        <link href="https://davidwilemski.com/feeds/all.atom.xml" rel="self"/>
        <id>https://davidwilemski.com/</id>
        <updated>{{updated_date}}</updated>
        {% for post in posts %}
          <entry>
          <title>{% if post.bookmark_of %}🔖 {% endif %}{{ post.title }}</title>
          <link href="/{{post.slug}}" rel="alternate"/>
          <published>{{ post.published }}</published>
          <updated>{{ post.updated }}</updated>
          <author>
            <name>David Wilemski</name>
          </author>
          <id>tag:davidwilemski.com,{{ post.date.date }}:{{ post.slug }}</id>
          <content type="html" xml:lang="en">
            {{ post.content | safe}}
            {% if post.bookmark_of %}
            <br />
            <a href="{{ post.bookmark_of }}" rel="nofollow">(🔖 bookmark)</a>
            {% endif %}
          </content>
          {% for tag in post.tags %}
          <category term="{{ tag }}"/>
          {% endfor %}
          </entry>
        {% endfor %}
        </feed>
        "#};
        let mut tera = Tera::default();
        tera.add_raw_template("atom.xml", atom_template)
            .expect("invalid atom template");

        Templates::new(Arc::new(tera), base_ctx)
    }

    pub fn add_context<T: Serialize + ?Sized>(&self, key: &str, val: &T) -> Templates {
        let mut new_ctx = self.ctx.clone();
        new_ctx.insert(key, val);

        Templates {
            tera: self.tera.clone(),
            ctx: new_ctx,
        }
    }

    pub fn render(&self, template: &str) -> Result<String> {
        self.tera
            .render(template, &self.ctx)
            .context("tera template rendering failed")
    }
}
