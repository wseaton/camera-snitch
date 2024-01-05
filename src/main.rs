use tokio::time::Duration;

use clap::Parser;
use futures_util::StreamExt;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

#[derive(Debug, PartialEq, Eq, Clone)]
enum CameraState {
    On,
    Off,
}

#[derive(Parser, Debug)]
struct Args {
    /// host of the MQTT server you are connecting to
    #[clap(long, default_value = "localhost")]
    mqtt_host: String,
    /// port of the MQTT server you are connecting to
    #[clap(long, default_value = "1883")]
    mqtt_port: u16,
    /// keepalive in seconds
    #[clap(long, default_value = "60")]
    mqtt_keepalive: u64,
    #[clap(long, default_value = "1000")]
    mqtt_pending_throttle: u64,

    /// debounce duration in milliseconds, tune this to what works on your system
    #[clap(long, default_value = "300")]
    debounce_duration: u64,

    /// loop duration in milliseconds
    #[clap(long, default_value = "10")]
    loop_duration: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let notify = inotify::Inotify::init()?;

    let files = glob::glob("/dev/video*")?;
    for file in files {
        tracing::info!("adding watcher for: {:?}", file);
        notify.watches().add(
            file?.to_str().unwrap(),
            inotify::WatchMask::OPEN | inotify::WatchMask::CLOSE,
        )?;
    }

    let mut buffer = [0u8; 4096];

    let mut mqttoptions = MqttOptions::new("camera-snitch", args.mqtt_host, args.mqtt_port);
    mqttoptions.set_keep_alive(Duration::from_secs(args.mqtt_keepalive));
    mqttoptions.set_pending_throttle(Duration::from_micros(args.mqtt_pending_throttle));

    tracing::info!("connecting to mqtt");
    let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    write_discovery(&mut client).await?;

    let mut last_state = CameraState::Off;

    let debounce_duration = Duration::from_millis(args.debounce_duration);
    let mut last_event_time = std::time::Instant::now() - debounce_duration;

    let mut stream = notify.into_event_stream(&mut buffer)?;

    loop {
        let mut current_state = last_state.clone();

        tokio::select! {
            Some(event) = stream.next() => {

                if let Ok(event) = event {
                    tracing::debug!("inotify event: {:?}", event);
                    match event.mask {
                        inotify::EventMask::OPEN => {
                            tracing::info!("camera opened");
                            current_state = CameraState::On;
                        }
                        inotify::EventMask::CLOSE_NOWRITE | inotify::EventMask::CLOSE_WRITE => {
                            tracing::info!("camera closed");
                            current_state = CameraState::Off;
                        }
                        _ => {}
                    }
                }

                // this is a simple debounce, we only send an event if the state has changed over the debounce window
                //
                // This is required because the camera will open and close multiple times when it is first plugged in or
                // opened by a browser and we don't want to send multiple events for that.
                if last_event_time.elapsed() >= debounce_duration && current_state != last_state {
                    send_event(&mut client, current_state.clone()).await?;
                    last_state = current_state;
                    last_event_time = std::time::Instant::now();
                }
            }
            Ok(notification) = eventloop.poll() => {
                match notification {
                    Event::Incoming(Incoming::Publish(p)) => {
                        tracing::debug!("received message: {:?}", p);
                    }
                    Event::Incoming(i) => {
                        tracing::debug!("received event: {:?}", i);
                    }
                    Event::Outgoing(o) => {
                        tracing::debug!("sent event: {:?}", o);
                    }
                }
            }
            else => {
                tracing::debug!("looping");
                tokio::time::sleep(Duration::from_millis(args.loop_duration)).await;
            }
        }
    }
}

#[tracing::instrument(skip(client))]
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
// https://www.home-assistant.io/docs/mqtt/discovery/
#[tracing::instrument(skip(client))]
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

    tracing::info!("publishing MQTT discovery paylod");
    if let Err(e) = client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await
    {
        tracing::error!("error publishing discovery: {}", e);
    }

    Ok(())
}
