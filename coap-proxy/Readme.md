# Proof of concept CoAP proxy

This receives the "pings" from the gateway, queries it's status and fowards this status to the backend server.

## Installation

pipx is needed to manage dependencies of the script. Follow the [installation guide](https://pipx.pypa.io/stable/installation/) for your platform.

On debian/ubuntu you can do:

```sh
sudo apt update
sudo apt install pipx
pipx ensurepath
```

## Configuration

### Generate a key

You need to generate a keypair for setting up the secure connection between the proxy and the gateway. Use the provided script:

```sh
pipx run generate.py
```

If you want to generate a new keypair, you first need to delete `server.cosekey`.

The command will output a public key, put this key for `kccs` in `../gateway/peers.yml`.

### Authentification to the backend

Copy the `.env.example` file to `.env` and complete it with the information (token and url) to push data to the backend.

### Port

By default the proxy listens on UDP port 5683, you can change it using the PORT env variable.

The gateway will connect to it through the internet, so this port needs to be open to the internet.

## Run the proxy

After setting the configuration, you can run the proxy using:

```sh
pipx run server.py
```
