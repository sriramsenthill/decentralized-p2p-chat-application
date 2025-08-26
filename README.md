# iroh-chat

A tiny, terminal-based group chat built on [iroh](https://github.com/n0-computer/iroh) + [`iroh-gossip`](https://github.com/n0-computer/iroh/tree/main/iroh-gossip).
One peer opens a topic and publishes a **ticket**; others join with that ticket and everyone gossips messages over QUIC.

---

## Features

* 🔑 One-line “ticket” you can copy/paste to invite peers
* 🗣️ Broadcast “about me” presence + plain text chat messages
* 👀 Neighbor up/down notifications and lag warnings
* 🧰 Minimal code using `tokio`, `clap`, `serde`, and `iroh-gossip`

---

## Getting started

### Prerequisites

* Rust (stable) + Cargo

  ```bash
  rustup update
  ```
* UDP allowed (iroh/QUIC uses UDP under the hood)

### Clone & build

```bash
git clone <your-repo-url> iroh-chat
cd iroh-chat
cargo build --release
```

> If you don’t have a `Cargo.toml` yet, add the dependencies below.

#### `Cargo.toml` (deps excerpt)

```toml
[package]
name = "iroh-chat"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
data-encoding = "2"
futures-lite = "2"
iroh = "0.29"          # or the latest compatible
iroh-gossip = "0.29"   # keep in sync with iroh
rand = "0.8"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

> Versions shown are examples—pin to the versions you use in your project.

---

## Usage

There are two subcommands:

* `open` — create a new topic and print a **ticket** others can use to join
* `join` — join an existing chat using a ticket

All commands accept an optional `--name <display-name>` (defaults to `user`).

### 1) Start a room

On **peer A**:

```bash
cargo run --release -- open --name Alice
```

Output (example):

```
> Ticket to join: e3ql4...  # copy this whole string
> Type messages and press enter to send...
```

### 2) Join the room

On **peer B** (or more), paste the ticket:

```bash
cargo run --release -- join --name Bob -- "<PASTE-TICKET-HERE>"
```

You should see joins and messages like:

```
> Neighbor connected: 7f3a…
> 7f3a… joined as Alice
> Type messages and press enter to send...
Alice: hello!
Bob: hi there 👋
```

> Tip: quotes around the ticket are recommended to avoid shell wrapping issues.

---

## How it works (quick tour)

* **Endpoint & discovery:**
  `Endpoint::builder().discovery_n0().bind().await?` creates a QUIC endpoint with local discovery.

* **Gossip:**
  `Gossip::builder().spawn(endpoint.clone())` starts the gossip instance; the `Router` accepts the gossip protocol (`iroh_gossip::ALPN`).

* **Ticket:**
  A ticket is a `TopicId` plus one or more `NodeAddr`s, JSON-encoded and **BASE32 (no padding)** encoded for easy sharing.

  * `open` creates a random `TopicId`, reads the local node’s address, and prints the ticket.
  * `join` decodes the ticket to learn the topic and bootstrap peers.

* **Subscribe & split:**
  `gossip.subscribe(topic_id, peer_ids).await?` returns a topic handle that’s split into `(sender, receiver)` for broadcasting and receiving.

* **Messages:**
  Messages are JSON with a small enum:

  ```rust
  enum MessageBody {
      AboutMe { from: NodeId, name: String },
      Message { from: NodeId, text: String },
  }
  ```

  A random 16-byte `nonce` is attached to avoid de-duplication across the network.

* **Events:**
  The receiver loop handles:

  * `Event::Received` → decode JSON and print joins/messages
  * `Event::NeighborUp/Down` → connectivity notices
  * `Event::Lagged` → buffer overflow warning

---

## Command reference

```bash
# Open a new chat, print a ticket
iroh-chat open [--name <name>]

# Join a chat with a ticket
iroh-chat join <ticket> [--name <name>]
```

Examples:

```bash
iroh-chat open --name Garden
iroh-chat join "kb6y...xyz" --name "Guest-1"
```

---

## Ticket format

* Human-pasteable string: `BASE32_NOPAD( JSON({ topic: TopicId, nodes: NodeAddr[] }) )`, lower-cased.
* Display/parse via `impl Display` and `impl FromStr` on `Ticket`.

---

## Troubleshooting

* **I don’t see peers connecting**

  * Make sure all machines can reach each other over UDP.
  * If behind strict NAT, try starting from a machine with a more permissive network and share that ticket.

* **Nothing prints after I type**

  * Press **Enter** to send; empty lines are ignored.
  * Check that both sides used the same ticket (no extra spaces).

* **Version mismatches**

  * Keep `iroh` and `iroh-gossip` crate versions in sync.

---

## Security notes

* This demo broadcasts **plaintext JSON** payloads inside the gossip protocol.
  For real applications, consider:

  * Authenticating peers
  * Encrypting message bodies end-to-end
  * Filtering/limiting message sizes and rates

---

## Project layout (this example)

```
src/
  main.rs        # all code shown in the snippet
Cargo.toml
README.md
```

