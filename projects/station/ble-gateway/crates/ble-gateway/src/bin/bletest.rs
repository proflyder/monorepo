//! Small D-Bus client for exercising the gateway on the station:
//! `bletest --destination :1.4 3 1` writes Commit,
//! `bletest --destination :1.4 --status 4` reads Status.

use std::collections::HashMap;
use std::process::ExitCode;

use anyhow::Context;
use ble_gateway::gatt::characteristic_path;
use clap::Parser;
use zbus::zvariant::Value;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "com.station.gateway")]
    destination: String,
    /// 0 SSID, 1 PSK, 2 token, 3 commit, 4 status.
    index: usize,
    value: Option<String>,
    #[arg(long)]
    status: bool,
}

#[rustfmt::skip]
#[zbus::proxy(interface = "org.bluez.GattCharacteristic1", default_service = "org.bluez")]
trait GattCharacteristic1 {
    fn write_value(&self, value: Vec<u8>, options: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    #[zbus(property)]
    fn value(&self) -> zbus::Result<Vec<u8>>;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    if let Err(cause) = run().await {
        eprintln!("{cause:#}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[rustfmt::skip]
async fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    let connection = zbus::Connection::system().await?;
    let proxy = GattCharacteristic1Proxy::builder(&connection)
        .destination(zbus::names::BusName::try_from(args.destination)?)?
        .path(characteristic_path(args.index))?
        .build()
        .await?;
    if args.status { println!("status={}", String::from_utf8_lossy(&proxy.value().await?).trim()); return Ok(()); }
    let value = args.value.context("a value is required unless --status is used")?;
    proxy.write_value(value.into_bytes(), HashMap::new()).await?;
    println!("write ok");
    Ok(())
}
