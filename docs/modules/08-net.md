# Module: nodalync-net

**Source:** Protocol Specification §11

## Overview

P2P networking using libp2p. Handles peer discovery, DHT, and message routing.

**Key Design Decisions:**

1. **Hash-Only Lookup for MVP:** The protocol supports hash-based content discovery only. 
   Keyword/semantic search is an application-layer concern and out of scope for the core protocol.
   Users discover content via external channels (social media, links, recommendations) and use
   the protocol to query by hash.

2. **DHT stores:** `content_hash -> AnnouncePayload` mapping. This allows anyone with a hash
   to find the content owner's addresses and metadata.

3. **No search index:** The DHT is NOT an inverted index. Future application-layer services
   can build search functionality on top of the protocol.

## Dependencies

- `nodalync-types` — All data structures
- `nodalync-wire` — Message encoding
- `nodalync-ops` — Operation handlers
- `libp2p` — P2P networking stack

---

## §11.1 Transport

```rust
pub fn build_transport(identity: &Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
    let tcp = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true));
    
    let transport = tcp
        .upgrade(Version::V1)
        .authenticate(noise::Config::new(&identity).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();
    
    transport
}
```

**Supported transports:**
- TCP (primary)
- QUIC (optional, for better performance)
- WebSocket (optional, for browser nodes)

**Security:**
- Noise protocol (XX handshake pattern)

**Multiplexing:**
- yamux (primary)
- mplex (fallback)

---

## §11.2 Discovery (DHT)

### Kademlia Configuration

```rust
pub fn build_kademlia(peer_id: PeerId) -> Kademlia<MemoryStore> {
    let mut config = KademliaConfig::default();
    config.set_query_timeout(Duration::from_secs(60));
    config.set_replication_factor(NonZeroUsize::new(DHT_REPLICATION).unwrap());
    
    let store = MemoryStore::new(peer_id);
    Kademlia::with_config(peer_id, store, config)
}

// Constants from spec
const DHT_BUCKET_SIZE: usize = 20;
const DHT_ALPHA: usize = 3;
const DHT_REPLICATION: usize = 20;
```

### Content Announcement

```rust
/// Announce content availability to DHT
/// Stores: content_hash -> AnnouncePayload
pub async fn dht_announce(&mut self, hash: &Hash, payload: AnnouncePayload) -> Result<()> {
    let key = Key::new(&hash.0);
    let value = encode_payload(&payload)?;
    
    self.kademlia.put_record(Record::new(key, value), Quorum::Majority).await?;
    
    Ok(())
}

/// Lookup content by hash (the ONLY lookup mechanism in protocol)
/// Returns owner's addresses and metadata if found
pub async fn dht_get(&mut self, hash: &Hash) -> Result<Option<AnnouncePayload>> {
    let key = Key::new(&hash.0);
    
    match self.kademlia.get_record(key).await {
        Ok(record) => {
            let payload: AnnouncePayload = decode_payload(&record.value)?;
            Ok(Some(payload))
        }
        Err(GetRecordError::NotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Remove content announcement from DHT
pub async fn dht_remove(&mut self, hash: &Hash) -> Result<()> {
    let key = Key::new(&hash.0);
    self.kademlia.remove_record(&key).await?;
    Ok(())
}
```

**Note on Search:**

The protocol does NOT include keyword search. The DHT only supports exact hash lookups.
Content discovery happens through application-layer mechanisms:

- External search services (could index L1 summaries)
- Social sharing (users share links containing hashes)
- Recommendations (applications can build on provenance data)
- Curated directories (third parties can maintain topic indexes)

This keeps the protocol minimal and focused on trustless content exchange.
```

---

## §11.3 Peer Discovery

### Bootstrap

```rust
const BOOTSTRAP_NODES: &[&str] = &[
    "/dns4/bootstrap1.nodalync.io/tcp/9000/p2p/12D3KooW...",
    "/dns4/bootstrap2.nodalync.io/tcp/9000/p2p/12D3KooW...",
];

pub async fn bootstrap(&mut self) -> Result<()> {
    for addr in BOOTSTRAP_NODES {
        let addr: Multiaddr = addr.parse()?;
        self.swarm.dial(addr)?;
    }
    
    // Bootstrap Kademlia
    self.kademlia.bootstrap()?;
    
    Ok(())
}
```

### Peer Exchange

```rust
/// Exchange peer lists with connected peers
pub async fn exchange_peers(&mut self) -> Result<()> {
    let my_peers: Vec<PeerInfo> = self.connected_peers()
        .iter()
        .map(|p| self.get_peer_info(p))
        .collect();
    
    for peer in self.connected_peers() {
        let msg = Message::new(
            MessageType::PeerInfo,
            encode_payload(&PeerInfoPayload {
                peer_id: self.peer_id(),
                public_key: self.public_key(),
                addresses: self.listen_addresses(),
                capabilities: self.capabilities(),
                content_count: self.content_count(),
                uptime: self.uptime(),
            })?,
            &self.identity,
        );
        self.send(&peer, msg).await?;
    }
    
    Ok(())
}
```

---

## §11.4 Message Routing

### Request-Response Protocol

```rust
#[derive(NetworkBehaviour)]
pub struct NodalyncBehaviour {
    kademlia: Kademlia<MemoryStore>,
    request_response: request_response::Behaviour<NodalyncCodec>,
    gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
}

pub struct NodalyncCodec;

impl request_response::Codec for NodalyncCodec {
    type Protocol = &'static str;
    type Request = Message;
    type Response = Message;
    
    fn protocol(&self) -> Self::Protocol {
        "/nodalync/1.0.0"
    }
    
    async fn read_request(&mut self, io: &mut impl AsyncRead) -> io::Result<Self::Request> {
        let bytes = read_length_prefixed(io, MAX_MESSAGE_SIZE).await?;
        decode_message(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    
    async fn write_response(&mut self, io: &mut impl AsyncWrite, msg: Self::Response) -> io::Result<()> {
        let bytes = encode_message(&msg)?;
        write_length_prefixed(io, &bytes).await
    }
}
```

### Send/Receive

```rust
/// Send message to specific peer
pub async fn send(&mut self, peer: &PeerId, message: Message) -> Result<Message> {
    let response = self.request_response
        .send_request(peer, message)
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    
    Ok(response)
}

/// Broadcast announcement via GossipSub
pub async fn broadcast(&mut self, message: Message) -> Result<()> {
    let topic = gossipsub::IdentTopic::new("/nodalync/announce/1.0.0");
    let bytes = encode_message(&message)?;
    
    self.gossipsub.publish(topic, bytes)?;
    
    Ok(())
}
```

### Timeouts and Retries

```rust
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: usize = 3;

pub async fn send_with_retry(&mut self, peer: &PeerId, message: Message) -> Result<Message> {
    let mut last_error = None;
    
    for attempt in 0..MAX_RETRIES {
        match timeout(MESSAGE_TIMEOUT, self.send(peer, message.clone())).await {
            Ok(Ok(response)) => return Ok(response),
            Ok(Err(e)) => {
                last_error = Some(e);
                // Exponential backoff
                tokio::time::sleep(Duration::from_millis(100 * 2_u64.pow(attempt as u32))).await;
            }
            Err(_) => {
                last_error = Some(Error::Timeout);
            }
        }
    }
    
    Err(last_error.unwrap())
}
```

---

## Network Trait

```rust
#[async_trait]
pub trait Network {
    // Discovery (hash-based only)
    async fn dht_announce(&mut self, hash: &Hash, payload: AnnouncePayload) -> Result<()>;
    async fn dht_get(&mut self, hash: &Hash) -> Result<Option<AnnouncePayload>>;
    async fn dht_remove(&mut self, hash: &Hash) -> Result<()>;
    
    // Messaging
    async fn send(&mut self, peer: &PeerId, message: Message) -> Result<Message>;
    async fn broadcast(&mut self, message: Message) -> Result<()>;
    
    // Specific message helpers
    async fn send_preview_request(&mut self, peer: &PeerId, hash: &Hash) -> Result<PreviewResponsePayload>;
    async fn send_query(&mut self, peer: &PeerId, request: QueryRequestPayload) -> Result<QueryResponsePayload>;
    async fn send_channel_open(&mut self, peer: &PeerId, request: ChannelOpenPayload) -> Result<ChannelAcceptPayload>;
    async fn send_channel_close(&mut self, peer: &PeerId, request: ChannelClosePayload) -> Result<ChannelClosePayload>;
    async fn broadcast_settlement_confirm(&mut self, confirm: SettleConfirmPayload) -> Result<()>;
    
    // Peer management
    fn connected_peers(&self) -> Vec<PeerId>;
    fn listen_addresses(&self) -> Vec<Multiaddr>;
    async fn dial(&mut self, addr: Multiaddr) -> Result<()>;
    
    // Event loop
    async fn next_event(&mut self) -> NetworkEvent;
}

pub enum NetworkEvent {
    MessageReceived { peer: PeerId, message: Message },
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    DhtPutComplete { key: Hash, success: bool },
    DhtGetResult { key: Hash, value: Option<Vec<u8>> },
}
```

---

## Test Cases

1. **Bootstrap**: Connect to bootstrap nodes
2. **DHT announce/lookup**: Announce content, find it from another node by hash
3. **DHT remove**: Remove announcement, no longer findable
4. **Request-response**: Send query, receive response
5. **Timeout**: Slow peer triggers timeout
6. **Retry**: Failed request retries
7. **Peer discovery**: Find peers through DHT
8. **GossipSub**: Broadcast reaches subscribers
9. **Channel messages**: Open/close flow works
10. **Settlement broadcast**: Confirm reaches all peers
