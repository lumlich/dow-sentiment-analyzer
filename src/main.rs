mod api;
mod decision;
mod disruption;
mod rolling;
mod sentiment;
mod engine;
mod history;

use shuttle_axum::ShuttleAxum;

#[shuttle_runtime::main]
async fn axum() -> ShuttleAxum {
    let router = api::create_router();
    Ok(router.into())
}
