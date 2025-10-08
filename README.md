# BLE scan gateway

This repository is a poof of concept showing how we can report the presence of BLE tags in the proximity of a MCU.

This version is using the Thingy91X development kit.

## Architecture

```mermaid
graph TD;
    B(BLE scan) --> nRF5340;
    G(GNSS location) --> nRF9151;
    nRF5340-->|UART + Postcard| nRF9151;
    nRF9151-->|LTE-M + HTTP| server;
```

### nRF5340 Network core

`net/` directory.

The network core of the nRF5340 MCU is scanning for BLE packets and sending them to the nRF9151 SiP using UART (VCOM1).

One task scans for BLE advertisement packets and store them in a list.  
The second task reads this list and writes it into the UART channel every 2 seconds. Using `postcard` encoding and applying `COBS` on top. Once the data is sent the list of scanned devices is cleared.

### nRF5340 Application core

`app/` directory.

The application core is not used in this project. It is only used to initialize the peripherals and start the network core.

### nRF9151

`gateway/` directory.

This chip is used to get the location of the board using GNSS and communicate using LTE-M.

It is responsible for aggregating the information and sending it to the server.

- A first task reads the UART (VCOM1) channel, decodes and stores the BLE devices listed by the nRF5340, a timestamp is attached to each address to record when it was last seen.
- The second task deletes addresses that have a timestamp older than 10 minutes. This removes devices that havent been detected for too long (out of range).
- A third task fetches the new position (latitude, longitude and altitude) reported by the GNSS sensor (new value approximately every second). If the position returned is valid it will be saved in a shared variable as the last know position.
- The last task sets up LTE-M networking and sends updates to the server every 60 seconds or when the button is pressed. The update is a `POST` request to the endpoint configured in `ENDPOINT_URL`, the body is the JSON serialization of `common_types::GatewayUpdate`, it contains the last known location and the list of devices that have been detected.

### The server

The server is run on the host PC and listens on all interfaces (for convenience) on port 4230, once it receives a POST request on `/mac`, the body of the request is decoded and printed to the console.

This server port has to be reachable by the developement kit's LTE-M network connection, the easiest is to have the port of the server exposed to the internet.

### Extras

- `common-types` contains the types that are sent through communication channels (UART, networking) and so are used in two programs.
- `reader` is a test application that reads the data sent through `UART` from the `net` application.

## Setup

### Flashing

We need to flash both cores of the nRF5340 and the nRF9151.

On the Thingy91X you will need to connect an external programmer through P8 or P9, provide power via the USB-C connector (J6), ensure the power switch (SW1) is in the "ON" position.

#### Network core

Set the SWD switch (SW2) to the "nRF53" posistion.

```sh
cd net
laze build -b nrf5340dk-net run
```

> If probe-rs complains about the core being locked up, add `-- --allow-erase-all` at the end of the command:
>
> ```sh
> laze build -b nrf5340dk-net run -- --allow-erase-all
> ```

Once you see that the program is started (showing `INFO` lines with the text `scanning...`) you can close the debugging session by pressing `ctrl + C` or closing the terminal.

#### Application core

Set the SWD switch (SW2) to the "nRF53" posistion.

```sh
cd app
laze build -b nrf5340dk run
```

#### nRF9151

Set the SWD switch (SW2) to the "nRF91" posistion.

```sh
cd gateway
laze build -b nordic-thingy-91-x-nrf9151 run
```

### Server

To receive the requests of the device and see its content you can use the sample web server, to run it do:

```sh
cd server
cargo run --release
```

New reports will appear like this (here the GNSS location was not obtained yet):

```log
Received update at 2025-10-07T14:19:29.882282308+02:00
Time of fix: None
Position: None
Altitude: None
Number of MAC addresses: 10
DE:3F:A6:0C:47:EC
CA:39:3F:DA:C3:6F
67:E0:A2:D3:69:4F
7A:3A:48:34:A6:E4
B2:21:7B:7E:C7:4D
DE:72:F4:4E:12:4F
E3:C7:71:84:B4:5A
B9:01:2B:8D:1F:4A
E4:93:47:2D:0D:94
F6:47:47:E4:22:6B
```

## Usage

### LED status

There is an RGB led on the Thingy91X, for now each color (red, green, blue) is used as individual LEDs to represent the status of different components.

- Red: first GNSS fix hasn't been acquired yet (location unknown)
- Blue: last data returned by the GNSS module was a valid location (updates every second)
- Green: sending update to the server using LTE-M.

Since those 3 colors are in the same package, two concurrent statuses can make different colors.

### Input

The top button can be pressed to force an update to be sent before the 60 seconds have been elapsed.
