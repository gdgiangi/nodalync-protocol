//! Transport layer for the Nodalync protocol.
//!
//! This module builds the libp2p transport stack using:
//! - DNS for hostname resolution
//! - TCP for connectivity
//! - Noise (XX handshake) for encryption
//! - Yamux for multiplexing
//! - Relay (optional) for NAT traversal

use libp2p::{core::upgrade, dns, identity::Keypair, noise, relay, tcp, yamux, PeerId, Transport};
use std::time::Duration;

/// Build the libp2p transport stack (without relay).
///
/// The transport stack consists of:
/// 1. DNS for resolving hostnames (dns4/dns6)
/// 2. TCP for base connectivity
/// 3. Noise protocol (XX handshake) for encryption
/// 4. Yamux for stream multiplexing
///
/// Returns a boxed transport suitable for use with a Swarm.
pub fn build_transport(
    keypair: &Keypair,
    idle_timeout: Duration,
) -> libp2p::core::transport::Boxed<(PeerId, libp2p::core::muxing::StreamMuxerBox)> {
    // Create TCP transport with nodelay for low latency
    let tcp_config = tcp::Config::default().nodelay(true);
    let tcp = tcp::tokio::Transport::new(tcp_config);

    // Wrap TCP with DNS resolution support
    let dns_tcp = dns::tokio::Transport::system(tcp).expect("DNS transport should initialize");

    // Create Noise config for authenticated encryption
    let noise_config = noise::Config::new(keypair).expect("noise keypair should be valid");

    // Create Yamux config for multiplexing
    let yamux_config = yamux::Config::default();

    // Build the transport stack
    dns_tcp
        .upgrade(upgrade::Version::V1)
        .authenticate(noise_config)
        .multiplex(yamux_config)
        .timeout(idle_timeout)
        .boxed()
}

/// Build the libp2p transport stack with relay support for NAT traversal.
///
/// This extends the base transport by adding relay client capabilities,
/// allowing the node to accept inbound connections through relay nodes
/// when behind a NAT.
///
/// Returns `(transport, relay_behaviour)` — the relay behaviour must be
/// passed to `NodalyncBehaviour::with_keypair_and_relay`.
pub fn build_transport_with_relay(
    keypair: &Keypair,
    idle_timeout: Duration,
) -> (
    libp2p::core::transport::Boxed<(PeerId, libp2p::core::muxing::StreamMuxerBox)>,
    relay::client::Behaviour,
) {
    // Create the relay client transport + behaviour
    let (relay_transport, relay_behaviour) =
        relay::client::new(keypair.public().to_peer_id());

    // Create TCP transport with nodelay for low latency
    let tcp_config = tcp::Config::default().nodelay(true);
    let tcp = tcp::tokio::Transport::new(tcp_config);

    // Wrap TCP with DNS resolution support
    let dns_tcp = dns::tokio::Transport::system(tcp).expect("DNS transport should initialize");

    // Combine TCP + relay transports: try direct TCP first, fall back to relay
    let combined = dns_tcp.or_transport(relay_transport);

    // Create Noise config for authenticated encryption
    let noise_config = noise::Config::new(keypair).expect("noise keypair should be valid");

    // Create Yamux config for multiplexing
    let yamux_config = yamux::Config::default();

    // Build the transport stack
    let transport = combined
        .upgrade(upgrade::Version::V1)
        .authenticate(noise_config)
        .multiplex(yamux_config)
        .timeout(idle_timeout)
        .boxed();

    (transport, relay_behaviour)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_transport() {
        let keypair = Keypair::generate_ed25519();
        let timeout = Duration::from_secs(30);

        // Should not panic
        let _transport = build_transport(&keypair, timeout);
    }

    #[test]
    fn test_build_transport_with_relay() {
        let keypair = Keypair::generate_ed25519();
        let timeout = Duration::from_secs(30);

        // Should not panic and return both transport and behaviour
        let (_transport, _relay_behaviour) = build_transport_with_relay(&keypair, timeout);
    }
}
