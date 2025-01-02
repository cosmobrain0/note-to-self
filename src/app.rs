use leptos::{either::Either, logging::log, prelude::*, task::spawn_local};
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment, WildcardSegment,
};

use crate::notebook::{Notebook, TextFile};

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/note-to-self.css"/>

        // sets the document title
        <Title text="Note to self"/>

        // content for this welcome page
        <Router>
            <main>
                <Routes fallback=move || "Not found.">
                    <Route path=StaticSegment("") view=HomePage/>
                    <Route path=WildcardSegment("any") view=NotFound/>
                </Routes>
            </main>
        </Router>
    }
}

#[cfg(feature = "ssr")]
async fn get_pool_from_context() -> Result<sqlx::Pool<sqlx::Postgres>, ServerFnError> {
    match use_context::<crate::AppState>() {
        Some(crate::AppState { pool }) => Ok(pool),
        None => Err(ServerFnError::ServerError(String::from(
            "Expected app state context",
        ))),
    }
}

#[server(prefix = "/api")]
async fn get_text(id: i32) -> Result<String, ServerFnError> {
    let text: String = match sqlx::query_as("SELECT text FROM texts WHERE id = $1")
        .bind(id)
        .fetch_one(&get_pool_from_context().await?)
        .await
    {
        Ok((text,)) => text,
        Err(e) => {
            return Err(ServerFnError::ServerError(e.to_string()));
        }
    };
    Ok(text)
}

#[server(prefix = "/api")]
async fn save_text(text: String, id: i32) -> Result<(), ServerFnError> {
    sqlx::query_as("UPDATE texts SET text = $1 WHERE id = $2")
        .bind(text)
        .bind(id)
        .fetch_optional(&get_pool_from_context().await?)
        .await
        .map(|_: Option<()>| ())
        .map_err(|e| ServerFnError::ServerError(e.to_string()))
}

#[server(prefix = "/api")]
async fn get_notebook(id: i32) -> Result<Notebook, ServerFnError> {
    Notebook::get_from_id(&get_pool_from_context().await?, id)
        .await
        .map_err(|e| ServerFnError::ServerError::<server_fn::error::NoCustomError>(e.to_string()))?
        .map(Ok)
        .unwrap_or_else(|| {
            Err(ServerFnError::ServerError(format!(
                "Couldn't find a notebook with id {id}!"
            )))
        })
}

#[server(prefix = "/api")]
async fn get_all_text_ids() -> Result<Vec<i32>, ServerFnError> {
    match sqlx::query_as("SELECT id FROM texts")
        .fetch_all(&get_pool_from_context().await?)
        .await
    {
        Ok(ids) => Ok(ids.into_iter().map(|(x,)| x).collect()),
        Err(e) => Err(ServerFnError::ServerError(e.to_string())),
    }
}

#[server(prefix = "/api")]
async fn save_notebook(notebook: Notebook) -> Result<(), ServerFnError> {
    println!("saving notebook!");
    notebook
        .save(&get_pool_from_context().await?)
        .await
        .map_err(|e| ServerFnError::ServerError::<server_fn::error::NoCustomError>(e.to_string()))
}

/// Renders the home page of your application.
#[component]
fn HomePage() -> impl IntoView {
    view! {
        <NotebookPage id=1 />
    }
}

#[component]
fn NotebookPage(id: i32) -> impl IntoView {
    let notebook = RwSignal::new(None);
    let text_ids = move || {
        notebook
            .with(|notebook| {
                notebook
                    .as_ref()
                    .map(|notebook: &Notebook| notebook.texts().map(|t| t.id()).collect::<Vec<_>>())
            })
            .unwrap_or_default()
    };
    Effect::new(move |_| {
        log!("Running the get notebook effect");
        spawn_local(async move {
            log!("spawn-local in the get notebook effect");
            if let Ok(received_notebook) = dbg!(get_notebook(id).await) {
                log!("Saving some notebook");
                notebook.set(Some(received_notebook));
            }
        })
    });
    Effect::new(move |_| {
        log!("Running an effect because of notebook update");
        notebook.with(|notebook| {
            log!("Notebook updated?");
            if let Some(notebook) = notebook.as_ref() {
                let notebook = notebook.clone();
                spawn_local(async move {
                    log!("About to save notebook!");
                    log!("{:#?}", &notebook);
                    save_notebook(notebook).await.unwrap();
                })
            }
        });
    });
    view! {
        <For
            each={text_ids}
            key={move |id| *id}
            children={move |id| view! {<TextInputCell id notebook />}}
        />
        <AddTextButton notebook />
    }
}

#[server(prefix = "/api")]
async fn add_new_text_to_notebook(id: i32) -> Result<TextFile, ServerFnError> {
    sqlx::query_as(
        "INSERT INTO texts (notebook_id, text) VALUES ($1, 'New Text Box...') RETURNING id, text",
    )
    .bind(id)
    .fetch_one(&get_pool_from_context().await?)
    .await
    .map_err(|e| ServerFnError::ServerError::<server_fn::error::NoCustomError>(e.to_string()))
    .map(|(id, text)| TextFile::new(id, text))
}

#[component]
fn AddTextButton(notebook: RwSignal<Option<Notebook>>) -> impl IntoView {
    let add_text = move || {
        if let Some(id) = notebook.with(|notebook| notebook.as_ref().map(|notebook| notebook.id()))
        {
            spawn_local(async move {
                match add_new_text_to_notebook(id).await {
                    Ok(text) => {
                        notebook.update(|notebook| notebook.as_mut().unwrap().add_new_text(text))
                    }
                    Err(e) => log!("Noooooo there was an error :( {:#?}", e),
                }
            })
        }
    };
    view! {
        <span id="add-text-button" on:click={move |_| add_text()}>"+"</span>
    }
}

#[server(prefix = "/api")]
async fn delete_text(id: i32) -> Result<bool, ServerFnError> {
    sqlx::query_as("DELETE FROM texts WHERE id = $1 RETURNING id")
        .bind(id)
        .fetch_all(&get_pool_from_context().await?)
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))
        .map(|ids: Vec<(i32,)>| !ids.is_empty())
}

#[server(prefix = "/api")]
async fn update_text(id: i32, text: String) -> Result<(), ServerFnError> {
    let _: Option<()> = sqlx::query_as("UPDATE texts SET text = $1 WHERE id = $2")
        .bind(text)
        .bind(id)
        .fetch_optional(&get_pool_from_context().await?)
        .await
        .map_err(|e| {
            ServerFnError::ServerError::<server_fn::error::NoCustomError>(e.to_string())
        })?;
    Ok(())
}

#[component]
fn TextInputCell(id: i32, notebook: RwSignal<Option<Notebook>>) -> impl IntoView {
    let active = RwSignal::new(false);
    let text = RwSignal::new(String::new());

    Effect::new(move |updated: Option<bool>| {
        if updated.is_some_and(|x| x) {
            true
        } else {
            notebook.with(|notebook| {
                if let Some(notebook) = notebook.as_ref() {
                    text.set(
                        notebook
                            .texts()
                            .find(|x| x.id() == id)
                            .unwrap()
                            .text()
                            .to_string(),
                    );
                    true
                } else {
                    false
                }
            })
        }
    });
    Effect::new(move |_| {
        log!("Activity changed");
        if !active.get() {
            log!("inactive");
            notebook.update(|notebook| {
                log!("{:#?}", &notebook);
                if let Some(notebook) = notebook.as_mut() {
                    notebook.set_text(id, text.get());
                }
            });
        }
    });
    let inner_active = move || {
        view! {
            <textarea
                prop:value=move || text.get()
                on:input:target=move |ev| text.set(ev.target().value())
            >
                {text.get_untracked()}
            </textarea>
        }
    };
    let inner_inactive = move || {
        let paragraph = NodeRef::<leptos::html::P>::new();
        paragraph.on_load(move |p| {
            Effect::new(move || {
                p.set_inner_text(text.get().as_str());
            });
        });
        view! {
            <p node_ref=paragraph></p>
        }
    };
    let save = move |_| {
        log!("Saving...");
        active.set(false);
    };
    let delete = move |_| {
        spawn_local(async move {
            notebook.update(|notebook| {
                if let Some(notebook) = notebook.as_mut() {
                    notebook.delete_text(id);
                }
            });
        });
    };
    let footer = move || {
        if active.get() {
            Either::Left(view! {
                <span on:click=save >
                    "Save"
                </span>
            })
        } else {
            Either::Right(view! {
                <span on:click=move |_| { active.set(true); }>
                    "Edit"
                </span>
                <span on:click=delete>
                    "Delete"
                </span>
            })
        }
    };
    view! {
        <div class="text-input-cell">
            <div class="text-input-cell-text">
                <Show when={move || active.get()} fallback={inner_inactive}>
                    {inner_active}
                </Show>
            </div>
            <div class="text-input-cell-footer">
                {footer}
            </div>
        </div>
    }
}

/// 404 - Not Found
#[component]
fn NotFound() -> impl IntoView {
    // set an HTTP status code 404
    // this is feature gated because it can only be done during
    // initial server-side rendering
    // if you navigate to the 404 page subsequently, the status
    // code will not be set because there is not a new HTTP request
    // to the server
    #[cfg(feature = "ssr")]
    {
        // this can be done inline because it's synchronous
        // if it were async, we'd use a server function
        let resp = expect_context::<leptos_actix::ResponseOptions>();
        resp.set_status(actix_web::http::StatusCode::NOT_FOUND);
    }

    view! {
        <h1>"Not Found"</h1>
    }
}
