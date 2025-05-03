use crate::{init_capnp, proxy_capnp};
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use std::path::Path;
use tokio::net::UnixStream;
use tokio::task;
use tokio_util::compat::*;
use futures::FutureExt;

pub async fn connect(
    socket_path: &Path,
) -> Result<(init_capnp::init::Client, proxy_capnp::thread::Client), Box<dyn std::error::Error>> {
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, writer) = stream.into_split();
    let reader = reader.compat();
    let writer = writer.compat_write();
    let network = Box::new(twoparty::VatNetwork::new(
        reader,
        writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));
    let mut rpc_system = RpcSystem::new(network, None);

    let init_client: init_capnp::init::Client =
        rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

    task::spawn_local(rpc_system.map(|_| ()));
    let construct_response = init_client.construct_request().send().promise.await?;
    let thread_map = construct_response.get()?.get_thread_map()?;
    let thread_response = thread_map.make_thread_request().send().promise.await?;
    let thread_client = thread_response.get()?.get_result()?;

    Ok((init_client, thread_client))
}