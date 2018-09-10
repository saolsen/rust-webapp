// Try and get a rust web app working.
// logging
// errors
// serve pages
// database
// deployment
// tracing
// metrics

#![recursion_limit = "1024"]

//#[macro_use]
//extern crate error_chain;

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
use actix_web::{http, middleware, server, App, HttpRequest, HttpResponse, AsyncResponder, FutureResponse, Path, State};

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
}

fn greet(req: &HttpRequest<AppState>) -> Box<Future<Item=String, Error=MailboxError>> {
    // this can panic, don't do that fucker.
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
}

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
            .resource("/", |r| r.route().a(greet))
            .resource("/{name}", |r| r.route().a(greet))
    }).bind(format!("0.0.0.0:{}", port))
        .expect("Can not bind to port")
        .run();

    Ok(())
}

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