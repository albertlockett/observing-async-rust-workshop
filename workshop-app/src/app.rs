use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::background;
use crate::model::AuditEvent;
use crate::routes;
use crate::services::{Catalog, Inventory};
use crate::state::AppState;

const AUDIT_QUEUE_CAPACITY: usize = 1_024;

pub(crate) async fn build(catalog_path: PathBuf) -> io::Result<Router> {
    let catalog = Catalog::load(catalog_path).await?;
    let inventory = Inventory::new(&catalog.all());
    let (audit_tx, audit_rx) = mpsc::channel::<AuditEvent>(AUDIT_QUEUE_CAPACITY);
    let state = AppState::new(catalog.clone(), inventory, audit_tx);

    background::start(catalog, audit_rx);
    Ok(routes::router(state))
}

pub(crate) async fn serve(listen: SocketAddr, catalog_path: PathBuf) -> io::Result<()> {
    let router = build(catalog_path).await?;
    let listener = TcpListener::bind(listen).await?;
    println!("listening on http://{}", listener.local_addr()?);
    println!("press Ctrl-C to stop");

    let handle = dial9::Dial9TokioHandle::current();
    dial9_utils::dial9_axum::axum_0_8::serve(listener, router.into_make_service())
        .with_executor(move |fut| {
            handle.spawn(fut);
        })
        .with_graceful_shutdown(shutdown_signal())
        .await

    // axum::serve(listener, router)
    //     .with_graceful_shutdown(shutdown_signal())
    // .await
}

async fn shutdown_signal() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        eprintln!("failed to listen for Ctrl-C: {err}");
    }
}
