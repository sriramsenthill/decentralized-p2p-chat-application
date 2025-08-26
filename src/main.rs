use anyhow::Result;
use clap::{Parser, Subcommand};
use data_encoding::BASE32_NOPAD;
use futures_lite::StreamExt; 
use iroh::{Endpoint, NodeAddr, NodeId, Watcher};
use iroh::protocol::Router;
use iroh_gossip::{net::Gossip, proto::TopicId};
use iroh_gossip::api::{GossipReceiver, Event};
use rand::random;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::io::{self};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

#[derive(Parser)]
#[command(name = "iroh-chat")]
struct Args {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, default_value = "user")]
    name: String,
}

#[derive(Subcommand)]
enum Commands {
    Open,
    Join { ticket: String },
}

#[derive(Debug, Serialize, Deserialize)]
enum MessageBody {
    AboutMe { from: NodeId, name: String },
    Message { from: NodeId, text: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    body: MessageBody,
    nonce: [u8; 16],  // To prevent deduplication
}

impl Message {
    fn new(body: MessageBody) -> Self {
        Self {
            body,
            nonce: random(),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Serialization failed")
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Ticket {
    topic: TopicId,
    nodes: Vec<NodeAddr>,
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let bytes = serde_json::to_vec(self).expect("Serialization failed");
        let text = BASE32_NOPAD.encode(&bytes).to_lowercase();
        write!(f, "{}", text)
    }
}

impl FromStr for Ticket {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = BASE32_NOPAD.decode(s.to_uppercase().as_bytes())?;
        serde_json::from_slice(&bytes).map_err(Into::into)
    }
}

async fn subscribe_loop(
    mut receiver: GossipReceiver,
    names: Arc<Mutex<HashMap<NodeId, String>>>,
) -> Result<()> {
    while let Some(event) = receiver.try_next().await? {
        match event {
            Event::Received(msg) => {
                let message = Message::from_bytes(&msg.content)?;
                let mut names = names.lock().await;
                match message.body {
                    MessageBody::AboutMe { from, name } => {
                        names.insert(from, name.clone());
                        println!("> {} joined as {}", from.fmt_short(), name);
                    }
                    MessageBody::Message { from, text } => {
                        let name = names.get(&from).cloned().unwrap_or(from.fmt_short());
                        println!("{}: {}", name, text);
                    }
                }
            }
            Event::NeighborUp(node_id) => {
                println!("> Neighbor connected: {}", node_id.fmt_short());
            }
            Event::NeighborDown(node_id) => {
                println!("> Neighbor disconnected: {}", node_id.fmt_short());
            }
            Event::Lagged => {
                println!("> Warning: Message queue lagged, some messages may have been lost");
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Create Iroh endpoint with discovery
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    // Build gossip instance (remove .await - it returns the instance directly)
    let gossip = Gossip::builder().spawn(endpoint.clone());

    // Set up router for handling gossip protocol
    let router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    // Generate or parse topic and peers based on role
    let (topic_id, peers) = match args.command {
        Commands::Open => {
            let topic_id = TopicId::from_bytes(random::<[u8; 32]>());
            // Get our own address without .await - node_addr() returns a Watcher
            let my_addr = endpoint.node_addr().initialized().await;
            let ticket = Ticket { topic: topic_id, nodes: vec![my_addr] };
            println!("> Ticket to join: {}", ticket);
            (topic_id, vec![])
        }
        Commands::Join { ticket } => {
            let ticket: Ticket = ticket.parse()?;
            (ticket.topic, ticket.nodes)
        }
    };

    // Add known peers to the endpoint
    for addr in &peers {
        endpoint.add_node_addr(addr.clone())?;
    }

    // Subscribe to topic and wait for peers to connect
    let topic = gossip.subscribe(topic_id, peers.iter().map(|a| a.node_id).collect()).await?;
    let (sender, receiver) = topic.split();

    // Brief wait for connections (helps in local testing)
    sleep(Duration::from_secs(2)).await;

    // Broadcast "about me" message
    let about_me = Message::new(MessageBody::AboutMe {
        from: endpoint.node_id(),
        name: args.name.clone(),
    });
    sender.broadcast(about_me.to_bytes().into()).await?;

    // Spawn receiver loop
    let names = Arc::new(Mutex::new(HashMap::new()));
    tokio::spawn(subscribe_loop(receiver, names.clone()));

    // Input loop for sending messages
    println!("> Type messages and press enter to send...");
    let stdin = io::stdin();
    for line in stdin.lines() {
        let text = line?;
        if text.trim().is_empty() { continue; }
        let msg = Message::new(MessageBody::Message {
            from: endpoint.node_id(),
            text,
        });
        sender.broadcast(msg.to_bytes().into()).await?;
    }

    // Shutdown
    router.shutdown().await?;
    Ok(())
}