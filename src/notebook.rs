#[cfg(feature = "ssr")]
use sqlx::Error;

use leptos::server_fn::serde::{Deserialize, Serialize};

// database:
// table notebooks
// id | name | password_hash

// table texts
// id | notebook_id | text

// this file models and abstracts the database.
/// This struct seems to have different meanings on the server side
/// and on the client side. Maybe this should be two different structs?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notebook {
    id: i32,
    name: String,
    texts: Vec<TextFile>,
}
#[cfg(feature = "ssr")]
impl Notebook {
    pub async fn get_from_id(
        pool: &sqlx::Pool<sqlx::Postgres>,
        id: i32,
    ) -> Result<Option<Self>, Error> {
        let results: Vec<(String, i32, String)> = sqlx::query_as("SELECT name, texts.id, texts.text FROM notebooks JOIN texts ON notebooks.id = texts.notebook_id WHERE notebooks.id=$1").bind(id)
            .fetch_all(pool).await?;
        Ok(if results.is_empty() {
            None
        } else {
            Some(Self {
                id,
                name: results[0].0.clone(),
                texts: results
                    .into_iter()
                    .map(|(_, id, text)| TextFile { id, text })
                    .collect(),
            })
        })
    }

    pub async fn save(&self, pool: &sqlx::Pool<sqlx::Postgres>) -> Result<(), Error> {
        println!("Hi there!");
        let _: Option<()> = sqlx::query_as("INSERT INTO notebooks (id, name) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name")
            .bind(self.id).bind(&self.name)
            .fetch_optional(pool).await?;

        let values = self
            .texts()
            .enumerate()
            .map(|(i, _)| {
                format!(
                    "(${left}, $1, ${right})",
                    left = i * 2 + 1 + 1,
                    right = i * 2 + 2 + 1
                )
            })
            .reduce(|acc, val| acc + ", " + val.as_str());
        if let Some(values) = values {
            let query_text = format!("INSERT INTO texts (id, notebook_id, text) VALUES {values} ON CONFLICT (id) DO UPDATE SET text = EXCLUDED.text");
            dbg!(&query_text);
            let mut query = sqlx::query_as(&query_text).bind(self.id);
            for text in self.texts() {
                query = query.bind(text.id).bind(text.text.as_str());
            }
            let _: Option<()> = query.fetch_optional(pool).await?;
        }

        let ids_to_keep = self
            .texts()
            .map(|t| t.id.to_string())
            .reduce(|acc, val| acc + ", " + val.as_str());
        if let Some(ids_to_keep) = ids_to_keep {
            let query_text =
                format!("DELETE FROM texts WHERE id NOT IN ({ids_to_keep}) AND notebook_id = $1");
            todo!("bind notebook_id to $1");
            todo!("perform the query");
            let mut query = sqlx::query_as(&query_text).bind(self.id);
        } else {
            let query_text = format!("DELETE FROM texts WHERE notebook_id = $1");
            todo!("bind botebook_id to $1");
            todo!("perform the query");
            let mut query = sqlx::query_as(&query_text).bind(self.id);
        }
        Ok(())
    }
}
impl Notebook {
    pub fn add_new_text(&mut self, text: TextFile) {
        self.texts.push(text);
    }

    pub fn set_text(&mut self, id: i32, text: String) {
        leptos::logging::log!("setting id {id} to '{text}' for notebook: {:#?}", &self);
        if let Some(text_file) = self.texts.iter_mut().find(|t| t.id == id) {
            text_file.text = text;
        }
    }

    pub fn delete_text(&mut self, id: i32) {
        if let Some(i) = self
            .texts
            .iter()
            .enumerate()
            .find(|(_, x)| x.id() == id)
            .map(|(i, _)| i)
        {
            self.texts.remove(i);
        }
    }
}
impl Notebook {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn texts(&self) -> std::slice::Iter<'_, TextFile> {
        self.texts.iter()
    }
    pub fn id(&self) -> i32 {
        self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextFile {
    text: String,
    id: i32,
}
#[cfg(feature = "ssr")]
impl TextFile {
    pub fn new(id: i32, text: String) -> Self {
        Self { id, text }
    }
}
impl TextFile {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn id(&self) -> i32 {
        self.id
    }
}
