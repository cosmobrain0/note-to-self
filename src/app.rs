use leptos::{either::Either, html::inner_html, logging::log, prelude::*, task::spawn_local};
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment, WildcardSegment,
};

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
    let text: String = match sqlx::query_as("SELECT text FROM text_files WHERE id = $1")
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
    sqlx::query_as("UPDATE text_files SET text = $1 WHERE id = $2")
        .bind(text)
        .bind(id)
        .fetch_optional(&get_pool_from_context().await?)
        .await
        .map(|_: Option<()>| ())
        .map_err(|e| ServerFnError::ServerError(e.to_string()))
}

#[server(prefix = "/api")]
async fn get_all_text_ids() -> Result<Vec<i32>, ServerFnError> {
    match sqlx::query_as("SELECT id FROM text_files")
        .fetch_all(&get_pool_from_context().await?)
        .await
    {
        Ok(ids) => Ok(ids.into_iter().map(|(x,)| x).collect()),
        Err(e) => Err(ServerFnError::ServerError(e.to_string())),
    }
}

/// Renders the home page of your application.
#[component]
fn HomePage() -> impl IntoView {
    let texts = RwSignal::new(vec![]);
    Effect::new(move |_| {
        spawn_local(async move {
            println!("hey");
            if let Ok(mut ids) = dbg!(get_all_text_ids().await) {
                ids.sort();
                println!("Hi!");
                texts.set(ids);
            }
        })
    });
    view! {
        <For
            each={move || texts.get()}
            key={move |id| *id}
            children={move |id| view! {<TextInputCell id texts />}}
        />
        <AddTextButton texts />
    }
}

#[server(prefix = "/api")]
async fn create_new_text() -> Result<i32, ServerFnError> {
    sqlx::query_as("INSERT INTO text_files (text) VALUES ('New text box...') RETURNING id")
        .fetch_one(&get_pool_from_context().await?)
        .await
        .map(|(id,)| id)
        .map_err(|e| ServerFnError::ServerError(e.to_string()))
}

#[component]
fn AddTextButton(texts: RwSignal<Vec<i32>>) -> impl IntoView {
    let add_text = move || {
        spawn_local(async move {
            if let Ok(id) = dbg!(create_new_text().await) {
                texts.update(|ids| {
                    ids.push(id);
                    ids.sort();
                })
            }
        })
    };
    view! {
        <span id="add-text-button" on:click={move |_| add_text()}>"+"</span>
    }
}

#[server(prefix = "/api")]
async fn delete_text(id: i32) -> Result<bool, ServerFnError> {
    sqlx::query_as("DELETE FROM text_files WHERE id = $1 RETURNING id")
        .bind(id)
        .fetch_all(&get_pool_from_context().await?)
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))
        .map(|ids: Vec<(i32,)>| !ids.is_empty())
}

#[component]
fn TextInputCell(id: i32, texts: RwSignal<Vec<i32>>) -> impl IntoView {
    log!("Rendering...");
    let initial_text = OnceResource::new(async move { get_text(id).await });
    let active = RwSignal::new(false);
    let text = RwSignal::new(String::new());

    Effect::new(move |_| {
        log!("running an effect!");
        if let Some(Ok(x)) = initial_text.get() {
            text.set(x);
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
        spawn_local(async move {
            let _ = save_text(text.get_untracked(), id).await;
            active.set(false);
        })
    };
    let delete = move |_| {
        spawn_local(async move {
            let _ = delete_text(id).await;
            if let Some(i) = texts.with_untracked(|t| {
                t.iter()
                    .enumerate()
                    .find(|(i, x)| **x == id)
                    .map(|(i, _)| i)
            }) {
                texts.update(|t| {
                    t.remove(i);
                });
            }
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
