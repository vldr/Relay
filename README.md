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

```relay <IP> <PORT>```

- `<IP>` is the IP address that should be bound to, for example: `127.0.0.1`
- `<PORT>` is the port that should be bound to, for example: `1234`

# Protocol

Relay uses the concept of rooms, which represent a list of clients that wish to send data between each other. A client can create a room and have other clients join the room. Once inside a room, data can be relayed.
- To create and join rooms, we use the *text-protocol*. 
- To send data between clients, we use the *binary-protocol*.

The **text-protocol** uses JSON to communicate between the client and the relay server to create and join rooms.

The **binary-protocol** uses a single byte at the start of your data to indicate the destination (when sending) and the source (when receiving).

**Example:**

To use the text-protocol in JavaScript, you would write:
```javascript
const webSocket = new WebSocket("<URL>");
webSocket.send(`{"type": "create"}`);
```

To use the binary-protocol in JavaScript, you would write:
```javascript
const webSocket = new WebSocket("<URL>");
webSocket.send(new Uint8Array(255, 1, 2, 3));
```

**Note:** Text can still be sent using the binary-protocol, it would just need to be wrapped in a Uint8Array or be sent using the binary opcode (if using a WebSocket library).

## Text Protocol

### `create` packet
Creates a new room.

**Note:** If the client fails to create a room, an "error" packet is sent as a response instead.

**Note:** A new room cannot be created while a client is already inside a room.

**Request:**

| Field | Type  | Description |
|-------|------|-------------|
| type  | `string` | The value should be "create". |
| size (optional) | `number` | Specifies the size of the room. The minimum value is *1*, the maximum value is *254*, and the default value is *2*. |

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

| Field | Type  | Description |
|-------|------|-------------|
| type  | `string` | The value will be "create". |
| id | `string` | The randomly generated identifier of the room, which is used to join the room by other clients. |

**Example:**
```json
{
    "type": "create",
    "id": "f4b087df-1e2c-4482-b434-d23b723cf6d"
}
```

### `join` packet
Joins a room.  

**Note:** If the client fails to join a room, an "error" packet is sent as a response instead.

**Note:** A room cannot be joined while a client is already inside a room.

**Request:**

| Field | Type  | Description |
|-------|------|-------------|
| type | string | The value should be "join". |
| id | string | The identifier of the room to join. |

**Example:**
```json
{
    "type": "join",
    "id": "f4b087df-1e2c-4482-b434-d23b723cf6d"
}
```

**Response:**  

| Field | Type  | Description |
|-------|------|-------------|
| type | `string` | The value will be "join". |
| size (conditional) | `string \| null` | **Important:** The client that sent the "join" packet will receive the number of clients that are in the room (excluding themselves). Everyone else in the room will receive the "join" packet with no size field. |

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

### `leave` packet
Indicates that a client has left a room. 

**Response:**

| Field | Type  | Description |
|-------|------|-------------|
| type  | `string` | The value will be "leave". |
| index | `number` | The index of the client that has left. |

**Example:**
```json
{
    "type": "leave",
    "index": 0
}
```

### `error` packet
Indicates that an error occurred when either joining or creating a room.  

**Note:** This packet is only sent during the creation or the joining of a room. You can assume that if a user gets this packet, then they're not in a room.

**Response:**

| Field | Type  | Description |
|-------|------|-------------|
| type  | `string` | The value will be "error". |
| message | `string` | A human-readable explanation of the error. |

**Example:**
```json
{
    "type": "error",
    "message": "The room does not exist."
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

When *sending*, the index byte indicates which client the packet should be sent to. 
- A value of *255* indicates a broadcast, which means the packet will be sent to everyone in the room (excluding the sender).
- A value between *0* and *254* indicates the index of the client that the packet will be sent to (a client can send to itself).

When *receiving*, the index byte will contain the index of the sender of the packet.
- A value between *0* and *254* indicates the index of the client that sent the packet.

**Data:** 

The data region contains *N* user-defined bytes, where *N* ≥ 0.

# Examples

Listed below are some example applications:

- [Chat](examples/chat)  
A simple chat application that shows how text messages can be sent between clients.
- [Cubic](https://github.com/vldr/Cubic/blob/master/Source/Network.cpp)  
A multiplayer WebGL voxel sandbox game.

# Building
**Note:** The following instructions assume that you are in a terminal (bash, cmd, etc).

1. Install [Rust](https://www.rust-lang.org/learn/get-started) and [Git](https://git-scm.com/).
2. Run `git clone https://github.com/vldr/relay`
3. Navigate to the cloned directory.
3. Run `cargo build --release`

After the build process finishes, the output executable will be located in the `target/release` folder.
