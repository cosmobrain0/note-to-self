use error::Error;
use leptos::*;
use serde::{Deserialize, Serialize};

// database:
// table notebooks
// id | name | password_hash

// table texts
// id | notebook_id | text

// this file models and abstracts the database.
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
}
impl Notebook {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn texts(&self) -> std::slice::Iter<'_, TextFile> {
        self.texts.iter()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextFile {
    text: String,
    id: i32,
}
#[cfg(feature = "ssr")]
impl TextFile {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn id(&self) -> i32 {
        self.id
    }
}
