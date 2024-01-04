use std::time::Duration;
use tokio::time::sleep;

use clap::Parser;

use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

#[derive(Debug, PartialEq, Eq, Clone)]
enum CameraState {
    On,
    Off,
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(long, default_value = "localhost")]
    mqtt_host: String,
    #[clap(long, default_value = "1883")]
    mqtt_port: u16,
    #[clap(long, default_value = "5")]
    loop_duration: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();

    let mut mqttoptions = MqttOptions::new("camera-snitch", args.mqtt_host, args.mqtt_port);
    mqttoptions.set_keep_alive(Duration::from_secs(30));

    tracing::info!("connecting to mqtt");
    let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    tracing::info!("send discovery message");
    write_discovery(&mut client).await?;

    let mut last_state = CameraState::Off;

    // loop every X seconds to check if camera is on, publish the state mesage if it has changed
    loop {
        match check_camera_state() {
            Ok(state) => {
                if state != last_state {
                    tracing::info!("camera state changed: {:?}", state);
                    last_state = state.clone();
                    send_event(&mut client, state).await?;
                }
            }
            Err(e) => {
                panic!("error checking camera state: {}", e)
            }
        };

        let notification = eventloop.poll().await;
        match notification {
            Ok(Event::Incoming(Incoming::Publish(p))) => {
                tracing::debug!("received message: {:?}", p);
            }
            Ok(Event::Incoming(i)) => {
                tracing::debug!("received event: {:?}", i);
            }
            Ok(Event::Outgoing(o)) => {
                tracing::debug!("sent event: {:?}", o);
            }
            Err(e) => {
                tracing::error!("error receiving notification: {}", e);
            }
        }

        sleep(Duration::from_secs(args.loop_duration)).await;
    }
}

async fn send_event(client: &mut AsyncClient, state: CameraState) -> anyhow::Result<()> {
    let topic = "homeassistant/binary_sensor/officecamera/state".to_string();
    let payload = match state {
        CameraState::On => "ON".to_string(),
        CameraState::Off => "OFF".to_string(),
    };

    let res = client
        .publish(&topic, QoS::AtLeastOnce, true, payload.clone())
        .await;

    match res {
        Ok(_) => tracing::info!("published state: {}", payload),
        Err(e) => tracing::error!("error publishing state: {}", e),
    }

    Ok(())
}

// implment mqtt sensor discovery for homeassistant for our binary sensor
async fn write_discovery(client: &mut AsyncClient) -> anyhow::Result<()> {
    let payload = serde_json::json!({
        "name": "OfficeCamera",
        "device": {
            "identifiers": ["officecamera"],
            "name": "Office Camera",
            "sw_version": "0.1",
            "model": "Custom Binary Sensor",
            "manufacturer": "Will Eaton <me@wseaton.com>"
        },
        "state_topic": "homeassistant/binary_sensor/officecamera/state",
        "device_class": "connectivity",
        "payload_on": "ON",
        "payload_off": "OFF",
    });

    let topic = "homeassistant/binary_sensor/officecamera/config".to_string();
    let payload = serde_json::to_string(&payload)?;

    if let Err(e) = client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await
    {
        tracing::error!("error publishing discovery: {}", e);
    }

    Ok(())
}

// TODO: is there a better more "event driven" and reliable way to do this?
// inotify? dbus?
fn check_camera_state() -> anyhow::Result<CameraState> {
    let res = std::process::Command::new("bash")
        .arg("-c")
        .arg("lsof -t /dev/video*")
        .output()?;

    tracing::debug!("lsof output: {}", String::from_utf8(res.stdout.clone())?);

    if res.stdout.is_empty() {
        Ok(CameraState::Off)
    } else {
        Ok(CameraState::On)
    }
}
