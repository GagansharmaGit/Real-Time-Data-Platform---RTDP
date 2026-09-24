use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use clap::{Parser, Subcommand, ValueEnum};
use rtdp_core::delivery::DeliveryMode;
use rtdp_core::recovery::RecoveryConfig;
use rtdp_core::serialization::FrameCodec;
use rtdp_transport_udp::{UdpPublisher, UdpPublisherConfig, UdpSubscriber, UdpSubscriberConfig};

#[derive(Parser, Debug)]
#[command(name = "rtdp", version = "0.1.0", about = "RTDP/1 Ultra-Low-Latency CLI Tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum CliDeliveryMode {
    BestEffort,
    Latest,
    AtLeastOnce,
    ReliableOrdered,
}

impl From<CliDeliveryMode> for DeliveryMode {
    fn from(m: CliDeliveryMode) -> Self {
        match m {
            CliDeliveryMode::BestEffort => DeliveryMode::BestEffort,
            CliDeliveryMode::Latest => DeliveryMode::Latest,
            CliDeliveryMode::AtLeastOnce => DeliveryMode::AtLeastOnce,
            CliDeliveryMode::ReliableOrdered => DeliveryMode::ReliableOrdered,
        }
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Publish messages to an RTDP subscriber
    Publish {
        #[arg(short = 't', long, default_value = "127.0.0.1:9876")]
        target: SocketAddr,

        #[arg(short = 'b', long, default_value = "0.0.0.0:0")]
        bind: SocketAddr,

        #[arg(short = 's', long, default_value_t = 1)]
        stream_id: u64,

        #[arg(short = 'p', long, default_value_t = 100)]
        publisher_id: u64,

        #[arg(short = 'm', long, default_value = "Hello RTDP")]
        message: String,

        #[arg(short = 'n', long, default_value_t = 1)]
        count: usize,

        #[arg(short = 'i', long, default_value_t = 100)]
        interval_ms: u64,
    },

    /// Subscribe to an RTDP stream
    Subscribe {
        #[arg(short = 'b', long, default_value = "0.0.0.0:9876")]
        bind: SocketAddr,

        #[arg(short = 'p', long, default_value = "127.0.0.1:9875")]
        publisher: SocketAddr,

        #[arg(short = 's', long, default_value_t = 1)]
        stream_id: u64,

        #[arg(short = 'm', long, value_enum, default_value_t = CliDeliveryMode::ReliableOrdered)]
        mode: CliDeliveryMode,

        #[arg(short = 'n', long)]
        max_messages: Option<usize>,
    },

    /// Inspect a binary RTDP/1 frame file
    Inspect {
        /// Path to raw frame file
        frame_file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Publish {
            target,
            bind,
            stream_id,
            publisher_id,
            message,
            count,
            interval_ms,
        } => {
            println!("Starting RTDP/1 Publisher to {}", target);
            let mut publ = UdpPublisher::new(UdpPublisherConfig {
                bind_addr: bind,
                target_addr: target,
                stream_id,
                publisher_id,
                ring_buffer_capacity: 10_000,
                max_frame_size: 1472,
                send_buffer_bytes: None,
            }).expect("Initialize publisher");

            println!("Publisher bound to {}", publ.local_addr());

            for i in 1..=count {
                let text = format!("{}: {}", message, i);
                let seq = publ.publish(text.as_bytes()).expect("Publish message");
                println!("[PUB] Sent seq={} bytes={} payload='{}'", seq, text.len(), text);

                if interval_ms > 0 && i < count {
                    std::thread::sleep(Duration::from_millis(interval_ms));
                }
            }
            println!("Publication finished ({} frames sent).", count);
        }

        Commands::Subscribe {
            bind,
            publisher,
            stream_id,
            mode,
            max_messages,
        } => {
            println!("Starting RTDP/1 Subscriber on {} (Stream={}, Mode={:?})", bind, stream_id, mode);
            let sub = UdpSubscriber::new(UdpSubscriberConfig {
                bind_addr: bind,
                publisher_addr: publisher,
                stream_id,
                mode: mode.into(),
                recovery_config: RecoveryConfig::default(),
                max_frame_size: 1472,
                channel_capacity: 10_000,
            }).expect("Initialize subscriber");

            let mut count = 0;
            loop {
                match sub.recv_timeout(Duration::from_secs(1)) {
                    Ok(frame) => {
                        count += 1;
                        let payload_str = String::from_utf8_lossy(&frame.payload);
                        println!(
                            "[SUB] seq={} pub={} ts={} len={} payload='{}'",
                            frame.header.sequence,
                            frame.header.publisher_id,
                            frame.header.timestamp_ns,
                            frame.payload.len(),
                            payload_str
                        );
                        if let Some(max) = max_messages {
                            if count >= max {
                                println!("Received {} messages. Exiting.", count);
                                break;
                            }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        // wait for more data
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }
        }

        Commands::Inspect { frame_file } => {
            println!("Inspecting RTDP/1 frame file: {:?}", frame_file);
            let mut data = std::fs::read(&frame_file).expect("Read frame file");
            let codec = FrameCodec::with_default_config();

            match codec.decode(&mut data) {
                Ok(view) => {
                    println!("============================================================");
                    println!(" Frame Header (Fixed 48 Bytes)");
                    println!("============================================================");
                    println!("  Magic         : 0x{:04X}", view.header.magic);
                    println!("  Version       : 0x{:02X}", view.header.version);
                    println!("  Flags         : 0x{:02X}", view.header.flags.0);
                    println!("  Message Type  : {:?} (0x{:04X})", view.header.message_type, view.header.message_type.as_u16());
                    println!("  Header Length : {} bytes", view.header.header_length);
                    println!("  Stream ID     : {}", view.header.stream_id);
                    println!("  Sequence      : {}", view.header.sequence);
                    println!("  Timestamp (ns): {}", view.header.timestamp_ns);
                    println!("  Publisher ID  : {}", view.header.publisher_id);
                    println!("  Payload Length: {} bytes", view.header.payload_length);
                    println!("  Checksum      : 0x{:08X} (CRC-32C Castagnoli: VALID)", view.header.checksum);
                    println!("------------------------------------------------------------");
                    println!(" Extensions: {} bytes", view.extension_bytes.len());
                    for (idx, ext) in view.extensions().enumerate() {
                        match ext {
                            Ok(e) => println!("    [{}] Type=0x{:04X}, Critical={}, Len={}", idx, e.ext_type, e.is_critical, e.value.len()),
                            Err(err) => println!("    [{}] Error parsing extension: {:?}", idx, err),
                        }
                    }
                    println!("------------------------------------------------------------");
                    println!(" Payload: {} bytes", view.payload.len());
                    let payload_preview = String::from_utf8_lossy(view.payload);
                    println!("  Content: {}", payload_preview);
                    println!("============================================================");
                }
                Err(e) => {
                    eprintln!("ERROR: Failed to decode frame: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
