<p align="center">
    <img src='logo.svg?raw=true'>
</p>

---

A fast and simple WebSocket relay, built in Rust, that enables a peer-to-peer-like network communication.

- [Getting Started](#getting-started)
- [Protocol](#protocol)
  - [Text Protocol](#text-protocol)
    - [`create` packet](#create-packet)
    - [`join` packet](#join-packet)
    - [`leave` packet](#leave-packet)
    - [`error` packet](#error-packet)
  - [Binary Protocol](#binary-protocol)
- [Examples](#examples)
- [Building](#building)

# Getting Started

### Binaries

To get started, you can either [build](#building) or download the precompiled binaries for your platform:

- [Windows](https://github.com/vldr/Relay/releases/download/master/relay-windows-amd64.exe)
- [Linux](https://github.com/vldr/Relay/releases/download/master/relay-linux-amd64)
- [MacOS](https://github.com/vldr/Relay/releases/download/master/relay-macos-amd64)

### Running

The following are the command-line arguments for the application:

`relay <IP> <PORT> <HOST>`

- `<IP>` is the IP address that should be bound to, for example: `127.0.0.1`
- `<PORT>` is the port that should be bound to, for example: `8080`
- `<HOST>` is the domain suffix of the [origin](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Origin) request header.
  - For example, using `example.com` will allow requests from `example.com`, `a.example.com`, and `a.b.example.com`, while requests that do not match this suffix will be rejected.
  - If left blank, then the origin header is not checked, and requests from any origin are accepted.

# Protocol

Relay uses the concept of rooms, which represent a list of clients that wish to send data between each other. A client can create a room and have other clients join the room. Once inside a room, data can be relayed.

- To create and join rooms, we use the _text protocol_.
- To send data between clients, we use the _binary protocol_.

The **text protocol** uses JSON to communicate between the client and the relay server to create and join rooms.

The **binary protocol** uses a single byte at the start of your data to indicate the destination (when sending) and the source (when receiving).

**Example:**

To use the text protocol in JavaScript, you would write:

```javascript
const webSocket = new WebSocket("<URL>");
webSocket.send(`{"type": "create"}`);
```

To use the binary protocol in JavaScript, you would write:

```javascript
const webSocket = new WebSocket("<URL>");
webSocket.send(new Uint8Array(255, 1, 2, 3));
```

**Note:** Text can still be sent using the binary protocol, it would just need to be wrapped in a Uint8Array or be sent using the binary opcode (if using a WebSocket library).

## Text Protocol

### `create` packet

Creates a new room.

- When creating a room, if an error occurs, an [`error`](#error-packet) packet is sent as a response.

- You cannot create a room while you are already inside another room.

**Request:**

| Field           | Type     | Description                                                                                                         |
| --------------- | -------- | ------------------------------------------------------------------------------------------------------------------- |
| type            | `string` | The value should be "create".                                                                                       |
| size | `number \| undefined` | Specifies the size of the room. <br><br>The minimum value is _1_, the maximum value is _254_, and the default value is _2_. |

**Example:**

<table>
<tr>
<th>Creating a room of size 10</th>
<th>Creating a room of size 2 (default)</th>
</tr>
<tr>
<td>

```json
{
  "type": "create",
  "size": 10
}
```

</td>
<td>

```json
{
  "type": "create"
}
```

</td>
</tr>
</table>

**Response:**

| Field | Type     | Description                                                                                     |
| ----- | -------- | ----------------------------------------------------------------------------------------------- |
| type  | `string` | The value will be "create".                                                                     |
| id    | `string` | The UUID identifier of the room, which is used to join the room. |

**Example:**

```json
{
  "type": "create",
  "id": "f4b087df-1e2c-4482-b434-d23b723cf6d"
}
```

---

### `join` packet

Joins a room. 

- When joining a room, if an error occurs, an [`error`](#error-packet) packet is sent as a response.

- You cannot join a room while you are already inside another room.

**Request:**

| Field | Type     | Description                         |
| ----- | -------- | ----------------------------------- |
| type  | `string` | The value should be "join".         |
| id    | `string` | The UUID identifier of the room to join. |

**Example:**

```json
{
  "type": "join",
  "id": "f4b087df-1e2c-4482-b434-d23b723cf6d"
}
```

**Response:**

| Field              | Type             | Description                                                                                                                                                                                                        |
| ------------------ | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| type               | `string`         | The value will be "join".                                                                                                                                                                                          |
| size | `string \| undefined` | The client that sent the "join" packet will receive the number of clients currently in the room (excluding themselves). <br><br> All other clients in the room will receive the "join" packet without a size field. |

**Example:**

<table>
<tr>
<th>Sender</th>
<th>Everyone else</th>
</tr>
<tr>
<td>

```json
{
  "type": "join",
  "size": 4
}
```

</td>
<td>

```json
{
  "type": "join"
}
```

</td>
</tr>
</table>

---

### `leave` packet

Indicates that a client has left a room.

**Response:**

| Field | Type     | Description                            |
| ----- | -------- | -------------------------------------- |
| type  | `string` | The value will be "leave".             |
| index | `number` | The index of the client that has left. |

**Example:**

```json
{
  "type": "leave",
  "index": 0
}
```

---

### `error` packet

Indicates that an error occurred when either joining or creating a room.

- You can assume that if you get this packet, then you're not in a room.

**Response:**

| Field   | Type     | Description                                |
| ------- | -------- | ------------------------------------------ |
| type    | `string` | The value will be "error".                 |
| message | `"InvalidSize" \| "AlreadyExists" \| "DoesNotExist" \| "IsFull"` | `"InvalidSize"` <br>The size parameter in the [`create`](#create-packet) packet is not valid. <br><br> `"AlreadyExists"` <br> The room already exists. <br><br> `"DoesNotExist"` <br> The room does not exist. <br><br> `"IsFull"` <br>The room is full. |

**Example:**

```json
{
  "type": "error",
  "message": "DoesNotExist"
}
```

## Binary Protocol

<table>
    <thead>
        <tr>
            <th>0</th>
            <th>1</th>
            <th>2</th>
            <th>3</th>
            <th>...</th>
            <th>N</th>
        </tr>
    </thead>
    <tbody>
        <tr>
            <td>Index</td>
            <td colspan=5>Data</td>
        </tr>
    </tbody>
</table>

**Index:**

When _sending_, the index byte indicates which client the packet should be sent to.

- A value of _255_ indicates a broadcast, which means the packet will be sent to everyone in the room (excluding the sender).
- A value between _0_ and _254_ indicates the index of the client that the packet will be sent to (a client can send to itself).

When _receiving_, the index byte will contain the index of the sender of the packet.

- A value between _0_ and _254_ indicates the index of the client that sent the packet.

**Data:**

The data region contains _N_ user-defined bytes, where _N_ ≥ 0.

# Examples

[Cubic](https://github.com/vldr/Cubic)  
A multiplayer WebGL voxel sandbox game.

[Share](https://github.com/vldr/Share)  
A real-time, peer-to-peer file transfer platform.

[Chat](examples/chat)  
A simple chat application.

# Building

**Note:** The following instructions assume that you are in a terminal (bash, cmd, etc).

1. Install [Rust](https://www.rust-lang.org/learn/get-started) and [Git](https://git-scm.com/).
2. Run `git clone https://github.com/vldr/relay`
3. Navigate to the cloned directory.
4. Run `cargo build --release`

After the build process finishes, the output executable will be located in the `target/release` folder.
