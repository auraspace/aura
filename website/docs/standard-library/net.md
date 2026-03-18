---
title: Networking (Net)
sidebar_position: 5
---

# Net Module

The `std/net` module provides TCP socket and DNS resolution utilities.

## TCPStream Class

A client for making TCP connections.

### Static Methods

- `connect(host: string, port: number): TCPStream`: Connects to a remote host.

### Methods

- `read(size: number): string`: Reads data from the stream.
- `write(content: string): number`: Writes data to the stream.
- `close(): void`: Closes the connection.

## TCPServer Class

A server for listening and accepting TCP connections.

### Static Methods

- `listen(port: number): TCPServer`: Starts listening on a port.

### Methods

- `accept(): TCPStream`: Accepts an incoming connection.
- `close(): void`: Stops the server.

## DNS Class

### Static Methods

- `resolve(host: string): string`: Resolves a hostname to an IP address.

### Example

```aura
import { TCPStream } from "std/net.aura";

let stream = TCPStream.connect("example.com", 80);
stream.write("GET / HTTP/1.1\r\nHost: example.com\r\n\r\n");
let res = stream.read(1024);
print(res);
stream.close();
```
