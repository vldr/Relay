<p align="center">
    <img src='logo.svg?raw=true' width='50%'>
</p>

---

A fast and simple WebSocket relay, built in Rust, that enables a peer-to-peer-like network communication.

- [About](#about)
- [Getting Started](#about)
- [Protocol](#protocol)
    - [Text Protocol](#text-protocol)
    - [Binary Protocol](#binary-protocol)
- [Building](#building)

# About

# Getting Started

# Protocol

## Text Protocol

### `create` packet
Creates a new room. If the size field is not provided, then a room of size *2* will be created. 

**Note:** If the client fails to create a room, an "error" packet is sent as a response instead.

**Note:** A new room cannot be created whilst a client is already inside another room.

**Request:**

| Field | Type  | Description |
|-------|------|-------------|
| type  | `string` | The value should be "create". |
| size (optional) | `number` | Specifies the size of the room. The minimum value is *1*, the maximum value is *254*, and the default value is *2*. |

**Example:**
```json
{
    "type": "create",
    "size": 2
}
```

**Response:**  

| Field | Type  | Description |
|-------|------|-------------|
| type  | `string` | The value will be "create". |
| id | `string` | The randomly generated identifier of the room, which is used to join the room by others. |

**Example:**
```json
{
    "type": "create",
    "id": "f4b087df-1e2c-4482-b434-d23b723cf6d"
}
```

### `join` packet
Joins a room.  

**Note:** If the client fails to join the room, an "error" packet is sent as a response instead.

**Note:** A room cannot be joined whilst a client is already inside another room.

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
| size (conditional) | `string \| null` | **Important:** The client that sent the "join" packet will receive the number of people that are in the room (excluding themselves). Everyone else in the room will receive the "join" packet with no size field. |

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
Indicates that a person has left the room. 

**Response:**

| Field | Type  | Description |
|-------|------|-------------|
| type  | `string` | The value will be "leave". |
| index | `number` | Specifies the index of the user that has left. |

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
- A value between *0* and *254* indicates the index of the client that the packet will be sent to (a client can send to oneself).

When *receiving*, the index byte will contain the index of the sender of the packet.
- A value between *0* and *254* indicates the index of the client that sent the packet.

**Data:** 

The data region contains *N* user-defined bytes, where $N \in \mathbb{Z}, N \ge 0$.

# Building
**Note:** The following instructions assume that you are in a terminal (bash, cmd, etc).

1. Install [Rust](https://www.rust-lang.org/learn/get-started) and [Git](https://git-scm.com/).
2. Run `git clone https://github.com/vldr/relay`
3. Navigate to the cloned directory.
3. Run `cargo build --release`

After the build process finishes, the output executable will be located in the `target/release` folder.