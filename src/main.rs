// Try and get a rust web app working.
// logging
// errors
// serve pages
// database
// deployment
// tracing
// metrics

extern crate actix_web;

use std::env;
use actix_web::{server, App, HttpRequest, Responder};

fn get_server_port() -> u16 {
    env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080)
}

fn greet(req: &HttpRequest) -> impl Responder {
    let to = req.match_info().get("name").unwrap_or("World");
    format!("Hello {}!", to)
}

fn main() {
    let port = get_server_port();

    server::new(|| {
        App::new()
            .resource("/", |r| r.f(greet))
            .resource("/{name}", |r| r.f(greet))
    })
    .bind(format!("0.0.0.0:{}", port))
    .expect("Can not bind to port")
    .run()
}