#[macro_use]
extern crate serde_json;

use actix_web::{get, web, App, HttpServer, HttpResponse, Responder};
use handlebars::Handlebars;
use anyhow::Result;

#[get("/{name}/index.html")]
async fn index(web::Path(name): web::Path<String>) -> impl Responder {
    let template = include_str!("../res/index.hbs");
    let reg = Handlebars::new();
    let html = reg.render_template(template, &json!({"name": name}))
        .expect("render data is known. qed.");
    HttpResponse::Ok().body(html)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(index))
        .bind("0.0.0.0:8080")?
        .run()
        .await
}
