#![allow(non_snake_case)]

use std::str::FromStr;

use leptos::{
    either::{Either, EitherOf4},
    logging::log,
    prelude::*,
    tachys::dom::window,
    task::spawn_local,
};
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    hooks::{use_navigate, use_params},
    params::Params,
    path, NavigateOptions, StaticSegment, WildcardSegment,
};
use wasm_bindgen::{prelude::Closure, JsCast};

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
                    <Route path=path!("/notebook/:id") view=NotebookPage />
                    <Route path=WildcardSegment("any") view=NotFound/>
                </Routes>
            </main>
        </Router>
    }
}

#[cfg(feature = "ssr")]
async fn get_pool_from_context_with_custom_error_type<E>(
) -> Result<sqlx::Pool<sqlx::Postgres>, ServerFnError<E>> {
    match use_context::<crate::AppState>() {
        Some(crate::AppState { pool }) => Ok(pool),
        None => Err(ServerFnError::ServerError::<E>(String::from(
            "Expected app state context",
        ))),
    }
}

#[cfg(feature = "ssr")]
#[inline]
async fn get_pool_from_context() -> Result<sqlx::Pool<sqlx::Postgres>, ServerFnError> {
    get_pool_from_context_with_custom_error_type::<server_fn::error::NoCustomError>().await
}

#[derive(Debug, Clone, Copy)]
pub struct NoAccessToNotebookError;
impl std::fmt::Display for NoAccessToNotebookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Not authorised to access that notebook!")
    }
}
impl FromStr for NoAccessToNotebookError {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s == Self::to_string(&NoAccessToNotebookError {}).as_str() {
            Ok(Self)
        } else {
            Err(())
        }
    }
}

#[server(prefix = "/api")]
async fn get_notebook(id: i32) -> Result<Notebook, ServerFnError<NoAccessToNotebookError>> {
    let Ok(session): Result<actix_session::Session, _> = leptos_actix::extract().await else {
        return Err(ServerFnError::ServerError::<NoAccessToNotebookError>(
            "can't get session from request!".to_string(),
        ));
    };
    if dbg!(session
        .get("notebook_id")
        .expect("Should be able to get id from session"))
    .is_some_and(|notebook_id: i32| notebook_id == id)
    {
        Notebook::get_from_id(
            &get_pool_from_context_with_custom_error_type::<NoAccessToNotebookError>().await?,
            id,
        )
        .await
        .map_err(|e| ServerFnError::ServerError::<NoAccessToNotebookError>(e.to_string()))?
        .map(Ok)
        .unwrap_or_else(|| {
            Err(ServerFnError::ServerError::<NoAccessToNotebookError>(
                format!("Couldn't find a notebook with id {id}!"),
            ))
        })
    } else {
        leptos_actix::redirect("/");
        Err(ServerFnError::WrappedServerError(NoAccessToNotebookError))
    }
}

#[server(prefix = "/api")]
async fn save_notebook(notebook: Notebook) -> Result<(), ServerFnError> {
    println!("saving notebook! {:#?}", &notebook);
    let session: actix_session::Session = leptos_actix::extract().await?;
    if dbg!(session
        .get("notebook_id")
        .expect("Should be able to get id from session"))
    .is_some_and(|notebook_id: i32| notebook_id == notebook.id())
    {
        notebook
            .save(&get_pool_from_context().await?)
            .await
            .map_err(|e| {
                ServerFnError::ServerError::<server_fn::error::NoCustomError>(e.to_string())
            })
    } else {
        leptos_actix::redirect("/");
        Err(ServerFnError::ServerError(
            "You don't have access to that notebook!".to_string(),
        ))
    }
}

/// Renders the home page of your application.
#[component]
fn HomePage() -> impl IntoView {
    view! {
        <NotebookSelectionPage />
    }
}

#[server(prefix = "/api")]
async fn select_notebook(notebook_name: String) -> Result<(), ServerFnError> {
    use leptos_actix::extract;
    let session: actix_session::Session = extract().await?;
    let notebook_id: Option<i32> = sqlx::query_as("SELECT id FROM notebooks WHERE name = $1")
        .bind(notebook_name)
        .fetch_optional(&get_pool_from_context().await?)
        .await
        .map_err(|e| e.to_string())
        .map_err(ServerFnError::<server_fn::error::NoCustomError>::ServerError)?
        .map(|(x,)| x);
    if let Some(notebook_id) = notebook_id {
        session
            .insert("notebook_id", notebook_id)
            .expect("failed to set notebook id");
        leptos_actix::redirect(&format!("/notebook/{notebook_id}"));
        Ok(())
    } else {
        Err(ServerFnError::ServerError(String::from(
            "That notebook doesn't exist!",
        )))
    }
}

#[server(prefix = "/api")]
async fn create_notebook(notebook_name: String) -> Result<(), ServerFnError> {
    use leptos_actix::extract;
    let session: actix_session::Session = extract().await?;
    let pool = get_pool_from_context().await?;
    let already_exists = sqlx::query_as("SELECT id FROM notebooks WHERE name = $1")
        .bind(&notebook_name)
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::ServerError::<server_fn::error::NoCustomError>(e.to_string()))?
        .map(|_: (i32,)| true)
        .unwrap_or(false);
    if already_exists {
        Err(ServerFnError::ServerError(
            "That notebook already exists!".to_string(),
        ))
    } else {
        let (id,): (i32,) = sqlx::query_as("INSERT INTO notebooks (name) VALUES ($1) RETURNING id")
            .bind(notebook_name)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                ServerFnError::ServerError::<server_fn::error::NoCustomError>(e.to_string())
            })?;
        session
            .insert("notebook_id", id)
            .expect("failed to set notebook id");
        leptos_actix::redirect(&format!("/notebook/{id}"));
        Ok(())
    }
}

#[component]
fn NotebookSelectionPage() -> impl IntoView {
    let select_notebook = ServerAction::<SelectNotebook>::new();
    let select_notebook_result = select_notebook.value();
    let select_notebook_loading = select_notebook.pending();
    let select_notebook_loaded_time = RwSignal::new(None);
    Effect::new(move |_| {
        if select_notebook_loading.get() {
            let start = window()
                .performance()
                .expect("should be able to get performance api")
                .now();
            select_notebook_loaded_time.set(Some(start));
            spawn_local(async move {
                // std::thread::sleep(std::time::Duration::from_secs(5));
                let reset_time = Closure::<dyn Fn()>::new(move || {
                    if select_notebook_loaded_time
                        .try_get_untracked()
                        .flatten()
                        .is_some_and(|t| t == start)
                    {
                        select_notebook_loaded_time.set(None);
                    }
                });
                window()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        reset_time.as_ref().unchecked_ref(),
                        2000,
                    )
                    .expect("should be able to set timeout");
                reset_time.forget();
            })
        }
    });
    let select_form_output = move || {
        if select_notebook_loaded_time.get().is_some() {
            match select_notebook_result.get() {
                None => EitherOf4::A(view! { <p></p> }),
                Some(Ok(())) => EitherOf4::B(view! { <p> "Redirecting..." </p> }),
                Some(Err(ServerFnError::ServerError(e))) => {
                    EitherOf4::C(view! { <p class="error-message"> {e.to_string()} </p> })
                }
                Some(Err(e)) => {
                    EitherOf4::D(view! { <p class="error-message"> {e.to_string()} </p> })
                }
            }
        } else {
            EitherOf4::A(view! { <p></p> })
        }
    };

    let create_notebook = ServerAction::<CreateNotebook>::new();
    let create_notebook_result = create_notebook.value();
    let create_notebook_loading = create_notebook.pending();
    let create_notebook_loaded_time = RwSignal::new(None);
    Effect::new(move |_| {
        if create_notebook_loading.get() {
            let start = window()
                .performance()
                .expect("should be able to get performance api")
                .now();
            create_notebook_loaded_time.set(Some(start));
            spawn_local(async move {
                // std::thread::sleep(std::time::Duration::from_secs(5));
                let reset_time = Closure::<dyn Fn()>::new(move || {
                    if create_notebook_loaded_time
                        .try_get_untracked()
                        .flatten()
                        .is_some_and(|t| t == start)
                    {
                        create_notebook_loaded_time.set(None);
                    }
                });
                window()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        reset_time.as_ref().unchecked_ref(),
                        2000,
                    )
                    .expect("should be able to set timeout");
                reset_time.forget();
            })
        }
    });
    let create_form_output = move || {
        if create_notebook_loaded_time.get().is_some() {
            match create_notebook_result.get() {
                None => EitherOf4::A(view! { <p></p> }),
                Some(Ok(())) => EitherOf4::B(view! { <p> "Redirecting..." </p> }),
                Some(Err(ServerFnError::ServerError(e))) => {
                    EitherOf4::C(view! { <p class="error-message"> {e.to_string()} </p> })
                }
                Some(Err(e)) => {
                    EitherOf4::D(view! { <p class="error-message"> {e.to_string()} </p> })
                }
            }
        } else {
            EitherOf4::A(view! { <p></p> })
        }
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum FormType {
        Select,
        Create,
    }

    let form_type = RwSignal::new(FormType::Select);
    let choose_select_notebook = move |_| form_type.set(FormType::Select);
    let choose_create_notebook = move |_| form_type.set(FormType::Create);

    view! {
        <div id="notebook-page">
            <div id="notebook-page-options">
                <button on:click=choose_select_notebook class:active={move || form_type.get() == FormType::Select}> "Open an existing notebook" </button>
                <button on:click=choose_create_notebook class:active={move || form_type.get() == FormType::Create}> "Create a new notebook" </button>
            </div>

            <Show when={move || form_type.get() == FormType::Select}>
                <ActionForm action=select_notebook>
                    <h1> "Select a notebook" </h1>
                    <input type="text" id="notebook_name" name="notebook_name" placeholder="Notebook Name..." required />
                    <button type="submit"> "Search" </button>
                </ActionForm>
                {select_form_output}
            </Show>

            <Show when={move || form_type.get() == FormType::Create}>
                <ActionForm action=create_notebook>
                    <h1> "Create a notebook" </h1>
                    <input type="text" id="notebook_name" name="notebook_name" placeholder="Notebook Name..." required />
                    <button type="submit"> "Create" </button>
                </ActionForm>
                {create_form_output}
            </Show>
        </div>
    }
}

#[derive(Params, PartialEq, Eq)]
struct NotebookParams {
    id: Option<i32>,
}

#[component]
fn NotebookPage() -> impl IntoView {
    let params = use_params::<NotebookParams>();
    let result = move || match params
        .read()
        .as_ref()
        .ok()
        .and_then(|params| params.id.clone())
    {
        Some(id) => Either::Left(view! { <NotebookComponent id /> }),
        None => Either::Right(view! { <h1> "Notebook not found" </h1> }),
    };
    view! {
        {result}
    }
}

#[component]
fn NotebookComponent(id: i32) -> impl IntoView {
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
        let navigate = use_navigate();
        log!("Running the get notebook effect");
        spawn_local(async move {
            log!("spawn-local in the get notebook effect");
            match get_notebook(id).await {
                Ok(received_notebook) => {
                    log!("Saving some notebook");
                    notebook.set(Some(received_notebook));
                }
                Err(ServerFnError::WrappedServerError(NoAccessToNotebookError)) => {
                    (navigate)("/", NavigateOptions::default());
                }
                Err(_) => (), // not really sure what to do here?
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
    let session: actix_session::Session = leptos_actix::extract().await?;
    if session
        .get("notebook_id")
        .expect("should be able to get notebook id from session")
        .is_some_and(|notebook_id: i32| notebook_id == id)
    {
        sqlx::query_as(
            "INSERT INTO texts (notebook_id, text) VALUES ($1, 'New Text Box...') RETURNING id, text",
        )
        .bind(id)
        .fetch_one(&get_pool_from_context().await?)
        .await
        .map_err(|e| ServerFnError::ServerError::<server_fn::error::NoCustomError>(e.to_string()))
        .map(|(id, text)| TextFile::new(id, text))
    } else {
        leptos_actix::redirect("/");
        Err(ServerFnError::ServerError(
            "You don't have access to that notebook!".to_string(),
        ))
    }
}

#[component]
fn AddTextButton(notebook: RwSignal<Option<Notebook>>) -> impl IntoView {
    let add_text = move || {
        log!("{:#?}", notebook.get());
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

#[component]
fn TextInputCell(id: i32, notebook: RwSignal<Option<Notebook>>) -> impl IntoView {
    let active = RwSignal::new(false);
    let text = RwSignal::new(String::new());
    let size: RwSignal<Option<(i32, i32)>> = RwSignal::new(None);

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
    let textarea_ref = NodeRef::<leptos::html::Textarea>::new();
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
                style={move || if let Some(size) = size.get() { format!("width: {}px; height: {}px", size.0, size.1) } else { String::new() } + if active.get() { "" } else { "display: none;" }}
                node_ref=textarea_ref
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
            <p node_ref=paragraph style={move || if !active.get() { "" } else { "display: none;" }}></p>
        }
    };
    let save = move |_| {
        log!("Saving...");

        if let Some(elmt) = textarea_ref.get_untracked() {
            size.set(Some((elmt.offset_width(), elmt.offset_height())));
        }
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
