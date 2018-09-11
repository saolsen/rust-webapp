// Try and get a rust web app working.
// logging
// errors
// serve pages
// database
// deployment
// tracing
// metrics

// Need to figure out how async handlers work and get the postgres stuff working too.

#![recursion_limit = "1024"]

//#[macro_use]
//extern crate error_chain;
#[macro_use]
extern crate serde_derive;
extern crate actix;
extern crate actix_web;
extern crate diesel;
extern crate dotenv;
extern crate futures;

//use errors::*;

/* mod errors {
    error_chain! { 
        foreign_links {
            MailboxError(::actix::MailboxError);
        }
    }
} */

use std::env;

use actix::prelude::*;
use actix_web::{http, middleware, server, App, Responder, HttpRequest, HttpResponse, AsyncResponder, FutureResponse, Path, State, http::ContentEncoding, Form, Result};

use diesel::prelude::*;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use futures::Future;

use dotenv::dotenv;

pub type ConnectionType = PgConnection;
pub type DbConnection = PooledConnection<ConnectionManager<ConnectionType>>;
pub type DbPool = Pool<ConnectionManager<ConnectionType>>;

/* pub fn connection(pool: &DbPool) -> Result<DbConnection, &'static str> {
    Ok(pool.get()?)
} */

pub struct DbExecutor(pub Pool<ConnectionManager<PgConnection>>);

impl Actor for DbExecutor {
    type Context = SyncContext<Self>;
}

struct User {
    name: String,
}

struct CreateUser {
    name: String,
}

// @TODO: Use the error thing.
impl Message for CreateUser {
    type Result = Result<User, String>;
}

impl Handler<CreateUser> for DbExecutor {
    type Result = Result<User, String>;

    fn handle(&mut self, msg: CreateUser, _: &mut Self::Context) -> Self::Result {
        // Do the database shit here.
        /* diesel::insert_into(users)
            .values(&new_user)
            .execute(&self.0)
            .expect("Error inserting person"); */

        Ok(User{name: "Steve".to_string()})
    }
}

struct AppState {
    db: Addr<DbExecutor>,
    store: Arc<Mutex<Vec<String>>>
}

// So lets start with like 4 pages. I should also be able to maybe do this with just in memory state.

// show widgets
// add widget form
// delete widget

/* fn get_widgets() -> impl Future<Item=u32, Error = Box<Error>> {
    Ok(100)
}

fn show_widgets(req: &HttpRequest<AppState>) -> Box<Future<Item=String, Error=String>> {
    get_widgets().responder()
} */

// can we just do impl AsyncResponder too?
fn html(body: &str) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html")
        .header("X-Hdr", "sample")
        .body(format!("<html><head><title>widget service</title></head><body>{}</body></html>", body))
}

fn index(req: &HttpRequest<AppState>) -> impl Responder {
    html("<a href=\"/widgets\">widgets</a></body></html>")
}

fn widget(name: &str) -> String {
    format!(r#"<li>{}<form action="/delete_widget" method="post"><input type="hidden" name="name" id="name" value="{}"><input type="submit" value="Delete"></form></li>"#, name, name)
}

fn get_widgets(req: &HttpRequest<AppState>) -> impl Responder {
    let widgets;
    {
        let guard = req.state().store.lock().unwrap();
        widgets = guard.iter().map(|s| widget(s)).collect::<Vec<String>>().concat();
    }
    html(format!(r#"
    <ul>
        {}
    </ul>
    <form action="/create_widget" method="post">
        name:<br>
        <input type="text" name="name"><br>
        <input type="submit" value="Create Widget">
    </form>
    "#, widgets).as_str())
}

#[derive(Deserialize)]
pub struct NewWidget {
    name: String,
}

#[derive(Deserialize)]
pub struct DeleteWidget {
    name: String,
}

fn create_widget((params, state): (Form<NewWidget>, State<AppState>)) -> Result<HttpResponse> {
    {
        let mut widgets = state.store.lock().unwrap();
        widgets.push(params.name.clone());
    }
    Ok(HttpResponse::TemporaryRedirect().header("Location", "/widgets").body("redirecting"))
}

fn delete_widget((params, state): (Form<DeleteWidget>, State<AppState>)) -> Result<HttpResponse> {
    {
        let mut widgets = state.store.lock().unwrap();
        *widgets = widgets.clone().into_iter().filter(|w| *w != params.name).collect();
    }
    Ok(HttpResponse::TemporaryRedirect().header("Location", "/widgets").body("redirecting"))
}

fn greet(req: &HttpRequest<AppState>) -> Box<Future<Item=String, Error=MailboxError>> {
    // This can panic, don't do that.
    let name = &req.match_info()["name"];

    req.state().db.send(CreateUser{name: name.to_owned()})
        .from_err()
        .and_then(|res| {
            match res {
                Ok(user) => Ok(format!("Hello {}", user.name)),
                Err(_) => Ok("Goodbye".to_string())
            }
        })
        .responder()

    // the error returned by a handler has to be something that can be turned into an http response.
    // so if I wanna do custom errors I have to have a translator thing.
    // I should probably have a thing that dumps to sentry or logs or something.
}

use std::sync::{Arc, Mutex};

// @NOTE: Going to try a mutex protected list shared by all the threads.

fn run() -> Result<(), String> {
    let data = Arc::new(Mutex::new(vec!["one".to_string(), "two".to_string(), "three".to_string()]));

    dotenv().ok();
    let port = env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let sys = actix::System::new("webapp");

    let manager = ConnectionManager::<ConnectionType>::new(database_url);
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to create pool.");
    // why 3?
    let addr = SyncArbiter::start(3, move || DbExecutor(pool.clone()));

    // @TODO: Logger
    println!("Starting server on port {}", port);
    server::new(move || {
        App::with_state(AppState{db: addr.clone(), store: data.clone()})
            .resource("/", |r| r.route().f(index))
            .resource("/widgets", |r| r.f(get_widgets))
            .resource("/create_widget", |r| {
                r.method(http::Method::POST).with(create_widget)
            })
            .resource("/delete_widget", |r| {
                r.method(http::Method::POST).with(delete_widget)
            })
    }).bind(format!("0.0.0.0:{}", port))
        .expect("Can not bind to port")
        .run();

    Ok(())
}

// @TODO: Check out askama for typed compile time templates for really fast templating! WOW!


fn main() {
    run();
    /* if let Err(ref e) = run() {
        println!("error: {}", e);

        for e in e.iter().skip(1) {
            println!("caused by: {}", e);
        }

        if let Some(backtrace) = e.backtrace() {
            println!("backtrace: {:?}", backtrace);
        }

        ::std::process::exit(1);
    } */
}