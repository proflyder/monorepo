//! Small D-Bus client used to exercise the gateway on the station.
//!
//! Usage: `BLE_DEST=:1.x bletest <char-index> <value> [status]`

use std::collections::HashMap;
use std::process::ExitCode;

use zbus::zvariant::OwnedValue;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let connection = match zbus::Connection::system().await {
        Ok(connection) => connection,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    if args.len() < 2 {
        println!("usage: BLE_DEST=:1.x bletest <char-index> <value> [status]");
        return ExitCode::from(2);
    }
    let destination = std::env::var("BLE_DEST").unwrap_or_else(|_| "com.station.gateway".into());
    let path = format!("/com/station/gateway/service0/char{}", args[0]);
    let interface = zbus::names::InterfaceName::try_from("org.bluez.GattCharacteristic1")
        .expect("valid interface");

    if args.get(2).map(String::as_str) == Some("status") {
        let properties =
            match zbus::fdo::PropertiesProxy::new(&connection, destination.as_str(), path.as_str())
                .await
            {
                Ok(properties) => properties,
                Err(err) => {
                    eprintln!("{err}");
                    return ExitCode::FAILURE;
                }
            };
        match properties.get(interface, "Value").await {
            Ok(value) => {
                let bytes = Vec::<u8>::try_from(value).unwrap_or_default();
                println!("status={}", String::from_utf8_lossy(&bytes).trim());
                return ExitCode::SUCCESS;
            }
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::FAILURE;
            }
        }
    }

    let proxy = match zbus::Proxy::new(
        &connection,
        destination.as_str(),
        path.as_str(),
        interface.as_str(),
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let value = args[1].as_bytes().to_vec();
    let options: HashMap<String, OwnedValue> = HashMap::new();
    match proxy.call_method("WriteValue", &(value, options)).await {
        Ok(_) => {
            println!("write ok");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
