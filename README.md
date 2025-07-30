# Bluetooth reporter demo

This repository is a poof of concept showing how we can report the presence of BLE tags in the proximity of a MCU.

This version is using the nRF5340dk, tested with the Thingy91X development kit.

## Setup

### Flashing

We need to flash both cores of the nRF5340.

On the Thingy91X you will need to connect an external programmer through P8 or P9, provide power via the USB-C connector (J6), ensure the power switch (SW1) is in the "ON" position and the SWD switch (SW2) is in the "nRF53" posistion.

#### Network core

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

```sh
cd net
laze build -b nrf5340dk run
```

### Network communication

Connect the USB port of the nRF5340 to your computer, on the Thingy91X it's the USB-C port (J6). Your computer should detect a new network interface. You can list them using `ip a`:

```
$ ip a
1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN group default qlen 1000
    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00
    inet 127.0.0.1/8 scope host lo
       valid_lft forever preferred_lft forever
    inet6 ::1/128 scope host noprefixroute 
       valid_lft forever preferred_lft forever
2: wlan0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP group default qlen 1000
    link/ether 24:eb:16:d2:b3:e2 brd ff:ff:ff:ff:ff:ff
    inet 192.168.1.16/24 brd 192.168.1.255 scope global dynamic noprefixroute wlan0
       valid_lft 82194sec preferred_lft 82194sec
    inet6 2a01:cb04:4d8:4400:785d:939a:8ca3:313e/64 scope global dynamic noprefixroute 
       valid_lft 1774sec preferred_lft 574sec
    inet6 fe80::e75b:b001:155c:674c/64 scope link noprefixroute 
       valid_lft forever preferred_lft forever
14: enp0s20f0u2u4: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel state UP group default qlen 1000
    link/ether c2:ee:05:af:55:c5 brd ff:ff:ff:ff:ff:ff
    altname enxc2ee05af55c5
```

Here the interface created by the development board is `enp0s20f0u2u4`, it has no configured IPv4 addresses.

The application has a pre-configured address of `10.42.0.61` and expects to send the data to `10.42.0.1`, we can set our PC to assume this address using the `iproute2` utility:

```sh
sudo ip addr add 10.42.0.1/24 dev enp0s20f0u2u4    
```

Replace `enp0s20f0u2u4` with the interface name you found earlier.

#### NetworkManager

Modern distributions use NetworkManager, setting the address using `iproute2` might not be enough, you will need to configure it using NetworkManager:

First, find the connection name attributed by NetworkManager to the interface/device using `nmcli con`:

```log
# nmcli con 
NAME                 UUID                                  TYPE       DEVICE        
Wired connection 1   ec2f8a54-4cf1-363d-959c-f7058c3be7aa  ethernet   enp0s20f0u2u4 
lo                   2d6db6c5-5c93-4e80-8564-57f6e423ef31  loopback   lo        
```

Here the connection name is `'Wired connection 1'`, quotes will be needed because of the spaces in the name.

To set the IPv4 address use this command:

```sh
sudo nmcli connection modify 'Wired connection 1' ipv4.addresses "10.42.0.1/24" ipv4.method "manual"
```

Replace `'Wired connection 1'` with your connection name.

### Web server

To receive the requests of the device and see its content you can use the sample web server, to run it do:

```sh
cd server
cargo run --release
```

New reports will appear like this:

```log
Received report: 768
MAC: 59 e5 f3 2e 92 c6 
MAC: 79 9e f7 f0 8f cf 
MAC: 51 8e 95 cd d0 07 
```
