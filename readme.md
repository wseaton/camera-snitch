# mqtt-camera-snitch

This is a simple daemon written in Rust that watches `/dev/video*` devices via `inotify` and pushes dat via MQTT to a binary sensor in homeassistant.

I'm currently using this to automate some key lights on my desk via a smart plug when the camera is used for meetings (to avoid constantly having to turn them on and off and having eye strain during focus time), but I'm sure there are other creative uses!

## Installation

```sh
cargo install --git https://github.com/wseaton/camera-snitch.git
```

## Usage

```sh
❯ camera-notifier --help
Usage: camera-notifier [OPTIONS]

Options:
      --mqtt-host <MQTT_HOST>
          host of the MQTT server you are connecting to [default: localhost]
      --mqtt-port <MQTT_PORT>
          port of the MQTT server you are connecting to [default: 1883]
      --mqtt-keepalive <MQTT_KEEPALIVE>
          keepalive in seconds [default: 60]
      --mqtt-pending-throttle <MQTT_PENDING_THROTTLE>
          [default: 1000]
      --debounce-duration <DEBOUNCE_DURATION>
          debounce duration in milliseconds, tune this to what works on your system [default: 300]
      --loop-duration <LOOP_DURATION>
          loop duration in milliseconds [default: 10]
  -h, --help
          Print help                                      Print help
```
