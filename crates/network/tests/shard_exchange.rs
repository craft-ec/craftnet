//! Integration tests for shard exchange between network nodes
//!
//! These tests verify that two nodes can connect and exchange shards
//! using the persistent stream protocol (libp2p-stream).

use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::swarm::SwarmEvent;
use tokio::time::timeout;
use futures::StreamExt;

use tunnelcraft_core::Shard;
use tunnelcraft_network::{
    TunnelCraftBehaviour, PeerId,
    SHARD_STREAM_PROTOCOL,
};

/// Create a test shard using the new onion format
fn create_test_shard() -> Shard {
    Shard::new(
        [1u8; 32],              // ephemeral_pubkey
        vec![2u8; 64],          // header (onion layers)
        b"test payload".to_vec(), // payload
        vec![3u8; 92],          // routing_tag
        0,                      // total_hops
        0,                      // hops_remaining
    )
}

/// Create a test swarm with stream protocol support
async fn create_test_swarm() -> (libp2p::Swarm<TunnelCraftBehaviour>, PeerId) {
    use libp2p::{noise, tcp, yamux, SwarmBuilder};

    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());

    let (behaviour, _relay_transport) = TunnelCraftBehaviour::new(peer_id, &keypair);

    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .unwrap()
        .with_behaviour(|_key, relay_behaviour| {
            Ok(TunnelCraftBehaviour {
                kademlia: behaviour.kademlia,
                identify: behaviour.identify,
                mdns: behaviour.mdns,
                gossipsub: behaviour.gossipsub,
                rendezvous_client: behaviour.rendezvous_client,
                rendezvous_server: behaviour.rendezvous_server,
                relay_client: relay_behaviour,
                dcutr: behaviour.dcutr,
                autonat: behaviour.autonat,
                stream: libp2p_stream::Behaviour::new(),
            })
        })
        .unwrap()
        .build();

    (swarm, peer_id)
}

/// Helper: connect two swarms and return them
async fn connect_swarms(
) -> (
    libp2p::Swarm<TunnelCraftBehaviour>,
    PeerId,
    libp2p::Swarm<TunnelCraftBehaviour>,
    PeerId,
) {
    let (mut swarm1, peer1) = create_test_swarm().await;
    let (mut swarm2, peer2) = create_test_swarm().await;

    swarm1.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
    swarm2.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();

    let addr1 = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm1.select_next_some().await {
            break address;
        }
    };

    let addr2 = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm2.select_next_some().await {
            break address;
        }
    };

    swarm1.behaviour_mut().add_address(&peer2, addr2);
    swarm2.behaviour_mut().add_address(&peer1, addr1);
    swarm1.dial(peer2).unwrap();

    // Wait for connection on BOTH sides
    timeout(Duration::from_secs(10), async {
        let mut s1 = false;
        let mut s2 = false;
        loop {
            tokio::select! {
                event = swarm1.select_next_some() => {
                    if matches!(event, SwarmEvent::ConnectionEstablished { .. }) {
                        s1 = true;
                        if s2 { return; }
                    }
                }
                event = swarm2.select_next_some() => {
                    if matches!(event, SwarmEvent::ConnectionEstablished { .. }) {
                        s2 = true;
                        if s1 { return; }
                    }
                }
            }
        }
    })
    .await
    .expect("Should connect within timeout");

    (swarm1, peer1, swarm2, peer2)
}

#[tokio::test]
async fn test_two_nodes_can_connect() {
    let (_swarm1, _peer1, _swarm2, _peer2) = connect_swarms().await;
    // If we get here, connection succeeded
}

#[tokio::test]
async fn test_stream_shard_exchange_direct() {
    // Test the stream frame protocol directly using connected libp2p streams
    use tunnelcraft_network::{
        read_frame, write_shard_frame, write_ack_frame, StreamFrame,
    };
    use futures::io::AsyncReadExt;

    let (mut swarm1, peer1, mut swarm2, peer2) = connect_swarms().await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Set up stream acceptance on node2
    let mut incoming2 = swarm2.behaviour().stream_control().accept(SHARD_STREAM_PROTOCOL).unwrap();

    // Open a stream from node1 to node2
    let mut control1 = swarm1.behaviour().stream_control();

    // Run swarms in background
    let s1 = tokio::spawn(async move {
        loop { let _ = swarm1.select_next_some().await; }
    });
    let s2 = tokio::spawn(async move {
        loop { let _ = swarm2.select_next_some().await; }
    });

    // Open stream and exchange shards
    let result = timeout(Duration::from_secs(10), async {
        // Open stream from node1 → node2
        let stream = control1.open_stream(peer2, SHARD_STREAM_PROTOCOL).await.unwrap();
        let (mut reader1, mut writer1) = AsyncReadExt::split(stream);

        // Accept stream on node2
        let (incoming_peer, stream2) = incoming2.next().await.unwrap();
        assert_eq!(incoming_peer, peer1);
        let (mut reader2, mut writer2) = AsyncReadExt::split(stream2);

        // Send shard from node1 → node2
        let shard = create_test_shard();
        write_shard_frame(&mut writer1, &shard, 1).await.unwrap();

        // Read shard on node2
        let frame = read_frame(&mut reader2).await.unwrap();
        let received_shard = match frame {
            StreamFrame::Shard { seq_id, shard: s } => {
                assert_eq!(seq_id, 1);
                s
            }
            _ => panic!("Expected Shard frame"),
        };

        assert_eq!(received_shard.ephemeral_pubkey, shard.ephemeral_pubkey);
        assert_eq!(received_shard.payload, shard.payload);
        assert_eq!(received_shard.routing_tag, shard.routing_tag);

        // Send ack from node2 → node1
        write_ack_frame(&mut writer2, 1, None).await.unwrap();

        // Read ack on node1
        let ack_frame = read_frame(&mut reader1).await.unwrap();
        match ack_frame {
            StreamFrame::Ack { seq_id, receipt } => {
                assert_eq!(seq_id, 1);
                assert!(receipt.is_none());
            }
            _ => panic!("Expected Ack frame"),
        }

        true
    })
    .await
    .expect("Should exchange shard via stream");

    assert!(result);
    s1.abort();
    s2.abort();
}

#[tokio::test]
async fn test_stream_shard_rejection() {
    use tunnelcraft_network::{
        read_frame, write_shard_frame, write_nack_frame, StreamFrame,
    };
    use futures::io::AsyncReadExt;

    let (mut swarm1, peer1, mut swarm2, peer2) = connect_swarms().await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut incoming2 = swarm2.behaviour().stream_control().accept(SHARD_STREAM_PROTOCOL).unwrap();
    let mut control1 = swarm1.behaviour().stream_control();

    let s1 = tokio::spawn(async move {
        loop { let _ = swarm1.select_next_some().await; }
    });
    let s2 = tokio::spawn(async move {
        loop { let _ = swarm2.select_next_some().await; }
    });

    let rejection_reason = timeout(Duration::from_secs(10), async {
        let stream = control1.open_stream(peer2, SHARD_STREAM_PROTOCOL).await.unwrap();
        let (mut reader1, mut writer1) = AsyncReadExt::split(stream);

        let (incoming_peer, stream2) = incoming2.next().await.unwrap();
        assert_eq!(incoming_peer, peer1);
        let (mut reader2, mut writer2) = AsyncReadExt::split(stream2);

        // Send shard
        let shard = create_test_shard();
        write_shard_frame(&mut writer1, &shard, 42).await.unwrap();

        // Read on node2
        let frame = read_frame(&mut reader2).await.unwrap();
        let seq_id = match frame {
            StreamFrame::Shard { seq_id, .. } => seq_id,
            _ => panic!("Expected Shard frame"),
        };
        assert_eq!(seq_id, 42);

        // Reject with nack
        write_nack_frame(&mut writer2, 42, "Invalid destination").await.unwrap();

        // Read nack on node1
        let nack_frame = read_frame(&mut reader1).await.unwrap();
        match nack_frame {
            StreamFrame::Nack { seq_id, reason } => {
                assert_eq!(seq_id, 42);
                reason
            }
            _ => panic!("Expected Nack frame"),
        }
    })
    .await
    .expect("Should receive rejection");

    assert_eq!(rejection_reason, "Invalid destination");
    s1.abort();
    s2.abort();
}

#[tokio::test]
async fn test_multiple_shards_via_stream() {
    use tunnelcraft_network::{
        read_frame, write_shard_frame, write_ack_frame, StreamFrame,
    };
    use futures::io::AsyncReadExt;

    let (mut swarm1, peer1, mut swarm2, peer2) = connect_swarms().await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut incoming2 = swarm2.behaviour().stream_control().accept(SHARD_STREAM_PROTOCOL).unwrap();
    let mut control1 = swarm1.behaviour().stream_control();

    let s1 = tokio::spawn(async move {
        loop { let _ = swarm1.select_next_some().await; }
    });
    let s2 = tokio::spawn(async move {
        loop { let _ = swarm2.select_next_some().await; }
    });

    timeout(Duration::from_secs(15), async {
        let stream = control1.open_stream(peer2, SHARD_STREAM_PROTOCOL).await.unwrap();
        let (mut reader1, mut writer1) = AsyncReadExt::split(stream);

        let (incoming_peer, stream2) = incoming2.next().await.unwrap();
        assert_eq!(incoming_peer, peer1);
        let (mut reader2, mut writer2) = AsyncReadExt::split(stream2);

        // Send 5 shards on the same stream (simulating 5/3 erasure coding)
        for i in 0..5u64 {
            let shard = Shard::new(
                [1u8; 32],
                vec![2u8; 64],
                format!("payload_{}", i).into_bytes(),
                vec![3u8; 92],
                0,
                0,
            );
            write_shard_frame(&mut writer1, &shard, i + 1).await.unwrap();
        }

        // Receive all 5 on node2 and ack each
        for i in 0..5u64 {
            let frame = read_frame(&mut reader2).await.unwrap();
            match frame {
                StreamFrame::Shard { seq_id, shard } => {
                    assert_eq!(seq_id, i + 1);
                    let expected_payload = format!("payload_{}", i).into_bytes();
                    assert_eq!(shard.payload, expected_payload);
                    write_ack_frame(&mut writer2, seq_id, None).await.unwrap();
                }
                _ => panic!("Expected Shard frame for seq_id={}", i + 1),
            }
        }

        // Receive all 5 acks on node1
        for i in 0..5u64 {
            let frame = read_frame(&mut reader1).await.unwrap();
            match frame {
                StreamFrame::Ack { seq_id, receipt } => {
                    assert_eq!(seq_id, i + 1);
                    assert!(receipt.is_none());
                }
                _ => panic!("Expected Ack frame for seq_id={}", i + 1),
            }
        }
    })
    .await
    .expect("Should exchange all 5 shards via stream");

    s1.abort();
    s2.abort();
}
