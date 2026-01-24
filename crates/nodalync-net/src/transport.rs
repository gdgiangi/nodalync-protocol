//! Transport layer for the Nodalync protocol.
//!
//! This module builds the libp2p transport stack using:
//! - TCP for connectivity
//! - Noise (XX handshake) for encryption
//! - Yamux for multiplexing

use libp2p::{
    core::upgrade,
    identity::Keypair,
    noise, tcp, yamux, PeerId, Transport,
};
use std::time::Duration;

/// Build the libp2p transport stack.
///
/// The transport stack consists of:
/// 1. TCP for base connectivity
/// 2. Noise protocol (XX handshake) for encryption
/// 3. Yamux for stream multiplexing
///
/// Returns a boxed transport suitable for use with a Swarm.
pub fn build_transport(
    keypair: &Keypair,
    idle_timeout: Duration,
) -> libp2p::core::transport::Boxed<(PeerId, libp2p::core::muxing::StreamMuxerBox)> {
    // Create TCP transport with nodelay for low latency
    let tcp_config = tcp::Config::default().nodelay(true);
    let tcp = tcp::tokio::Transport::new(tcp_config);

    // Create Noise config for authenticated encryption
    let noise_config = noise::Config::new(keypair).expect("noise keypair should be valid");

    // Create Yamux config for multiplexing
    let yamux_config = yamux::Config::default();

    // Build the transport stack
    tcp.upgrade(upgrade::Version::V1)
        .authenticate(noise_config)
        .multiplex(yamux_config)
        .timeout(idle_timeout)
        .boxed()
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
}
