//#![recursion_limit = "1024"]

//#[macro_use]
//extern crate error_chain;
#[macro_use]
extern crate serde_derive;
extern crate actix;
extern crate actix_web;
#[macro_use]
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
use actix_web::{error, http, middleware, server, App, Responder, HttpRequest, HttpResponse, AsyncResponder, FutureResponse, Path, State, http::ContentEncoding, Form, Result};

use diesel::prelude::*;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use futures::Future;

use dotenv::dotenv;

pub type ConnectionType = PgConnection;
pub type DbConnection = PooledConnection<ConnectionManager<ConnectionType>>;
pub type DbPool = Pool<ConnectionManager<ConnectionType>>;

// models
mod schema;
use schema::widgets;

#[derive(Queryable)]
pub struct Widget {
    pub name: String
}

#[derive(Insertable)]
#[table_name = "widgets"]
pub struct NewWidget<'a> {
    pub name: &'a str
}

// /models

// db service

pub struct DbExecutor(pub Pool<ConnectionManager<PgConnection>>);

impl Actor for DbExecutor {
    type Context = SyncContext<Self>;
}

struct CreateWidget {
    name: String,
}

impl Message for CreateWidget {
    type Result = Result<Widget, actix_web::Error>;
}

impl Handler<CreateWidget> for DbExecutor {
    type Result = Result<Widget, actix_web::Error>;

    fn handle(&mut self, msg: CreateWidget, _: &mut Self::Context) -> Self::Result {
        use schema::widgets::dsl::*;
        
        let new_widget = NewWidget{name: &msg.name};
        let conn = &*self.0.get().unwrap();

        diesel::insert_into(widgets)
            .values(&new_widget)
            .execute(conn)
            .map_err(|_| error::ErrorInternalServerError("Error inserting widget"))?;

        let mut items = widgets
            .filter(name.eq(&msg.name))
            .load::<Widget>(conn)
            .map_err(|_| error::ErrorInternalServerError("Error loading widget"))?;

        Ok(items.pop().unwrap())
    }
}

struct DeleteWidget {
    name: String,
}

impl Message for DeleteWidget {
    type Result = Result<(), actix_web::Error>;
}

impl Handler<DeleteWidget> for DbExecutor {
    type Result = Result<(), actix_web::Error>;

    fn handle(&mut self, msg: DeleteWidget, _: &mut Self::Context) -> Self::Result {
        use schema::widgets::dsl::*;

        let conn = &*self.0.get().unwrap();

        diesel::delete(widgets.filter(name.eq(msg.name)))
            .execute(conn)
            .map_err(|_| error::ErrorInternalServerError("Couldn't delete widget"))?;

        Ok(())
    }
}

struct GetWidgets;

impl Message for GetWidgets {
    type Result = Result<Vec<Widget>, actix_web::Error>;
}

impl Handler<GetWidgets> for DbExecutor {
    type Result = Result<Vec<Widget>, actix_web::Error>;

    fn handle(&mut self, msg: GetWidgets, _: &mut Self::Context) -> Self::Result {
        use schema::widgets::dsl::*;

        let conn = &*self.0.get().unwrap();

        let results = widgets.load::<Widget>(conn)
            .map_err(|_| error::ErrorInternalServerError("Error fetching widgets"))?;

        Ok(results)
    }
}

// /db service

struct AppState {
    db: Addr<DbExecutor>,
}

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

#[derive(Deserialize)]
pub struct NewWidgetForm {
    name: String,
}

#[derive(Deserialize)]
pub struct DeleteWidgetForm {
    name: String,
}

fn a_create_widget((state, params): (State<AppState>, Form<NewWidgetForm>),) -> Box<Future<Item=HttpResponse, Error=actix_web::Error>> {
    state.db.send(CreateWidget{name: params.name.clone()})
        .from_err()
        .and_then(|res| match res {
            Ok(_) => Ok(HttpResponse::TemporaryRedirect().header("Location", "/widgets").body("redirecting")),
            Err(_) => Ok(HttpResponse::InternalServerError().into())
        })
        .responder()
}

fn a_delete_widget((state, params): (State<AppState>, Form<DeleteWidgetForm>),) -> Box<Future<Item=HttpResponse, Error=actix_web::Error>> {
    state.db.send(DeleteWidget{name: params.name.clone()})
        .from_err()
        .and_then(|res| match res {
            Ok(_) => Ok(HttpResponse::TemporaryRedirect().header("Location", "/widgets").body("redirecting")),
            Err(_) => Ok(HttpResponse::InternalServerError().into())
        })
        .responder()
}

fn a_get_widgets(req: &HttpRequest<AppState>) -> Box<Future<Item=HttpResponse, Error=actix_web::Error>> {
    req.state().db.send(GetWidgets)
        .from_err()
        .and_then(|res| match res {
            Ok(widgets_data) => {
                let widgets = widgets_data.iter().map(|s| widget(&s.name)).collect::<Vec<String>>().concat();
                let body = html(format!(r#"
                <ul>
                    {}
                </ul>
                <form action="/create_widget" method="post">
                    name:<br>
                    <input type="text" name="name"><br>
                    <input type="submit" value="Create Widget">
                </form>
                "#, widgets).as_str());
                Ok(body)
            },
            Err(_) => Ok(HttpResponse::InternalServerError().into())
        })
        .responder()
}

// @NOTE: Going to try a mutex protected list shared by all the threads.

fn run() -> Result<(), String> {    
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
        App::with_state(AppState{db: addr.clone()})
            .resource("/", |r| r.route().f(index))
            //.resource("/widgets", |r| r.f(get_widgets))
            .resource("/widgets", |r| r.route().a(a_get_widgets))
            .resource("/create_widget", |r| {
                r.method(http::Method::POST).with(a_create_widget)
            })
            .resource("/delete_widget", |r| {
                r.method(http::Method::POST).with(a_delete_widget)
            })
    }).bind(format!("0.0.0.0:{}", port))
        .expect("Can not bind to port")
        .run();

    Ok(())
}

// @TODO: Check out askama for typed compile time templates for really fast templating! WOW!


fn main() {
    run().expect("Error");
}