use core::f64;
use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    path::PathBuf,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use clap::Parser;
use log::{debug, info, warn};
use mavlink_shouter::mavlink::{self, definitions::Offsets};
use rand::Rng;

// We're only creating messages without a signature
const PACKET_SIZE: usize = mavlink::v2::MAX_PACKET_LEN - mavlink::v2::SIGNATURE_LEN;

const NUM_MESSAGES: usize = 10000;

#[derive(Debug, Parser)]
struct Args {
    /// Number of threads to use
    #[arg(short, long, default_value = "2")]
    nr_threads: usize,
    /// Frequency of messages to send
    #[arg(short, long, default_value = "1000")]
    frequency: f64,
    /// Duration of the test in seconds
    #[arg(short, long, default_value = "10")]
    duration: u64,
    /// Whether to run sender and receiver on the same thread
    #[arg(short, long, default_value = "false")]
    same_thread: bool,
}

#[derive(Debug, Clone, Copy)]
struct Config {
    nr_threads: usize,
    frequency: f64,
    duration: Duration,
    same_thread: bool,
}

impl From<Args> for Config {
    fn from(args: Args) -> Self {
        Config {
            nr_threads: args.nr_threads,
            frequency: args.frequency,
            duration: Duration::from_secs(args.duration),
            same_thread: args.same_thread,
        }
    }
}

fn main() -> Result<()> {
    env_logger::builder()
        .format_module_path(false)
        .format_target(true)
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let config: Config = Args::parse().into();

    // Load the message definitions
    let definitions_path = PathBuf::from("mavlink/message_definitions/v1.0/ardupilotmega.xml");
    let definitions: Arc<_> = mavlink::definitions::try_get_offsets_from_xml(definitions_path)
        .map(|tbl| {
            tbl.into_iter()
                .filter(|(_, offsets)| offsets.component_id.is_some())
                .collect::<HashMap<_, _>>()
        })?
        .into();

    let ids: Arc<[u32]> = definitions.keys().copied().collect::<Vec<_>>().into();

    let now = Instant::now();

    // Start multiple threads to send messages
    let handles = (0..config.nr_threads)
        .map(|tid| {
            info!("Starting thread {}", tid);
            let ids = ids.clone();
            let definitions = definitions.clone();
            std::thread::spawn(move || record_round_trip_times(tid, ids, definitions, config))
        })
        .collect::<Vec<_>>();

    // Wait for all threads to finish, collect the round trip times and then print the stats
    let round_trip_times = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap().unwrap())
        .collect::<Vec<_>>();

    println!("Total time: {:.2} s", now.elapsed().as_secs_f64());

    print_stats(&round_trip_times);

    Ok(())
}

fn print_stats(round_trip_times: &[Duration]) {
    let n_rtt = round_trip_times.len();
    println!("Received {} round trip times", n_rtt);
    // Calculate the average round trip time
    let total_round_trip_time: u128 = round_trip_times.iter().map(|l| l.as_micros()).sum();
    let avg_round_trip_time = total_round_trip_time as f64 / n_rtt as f64;

    // Calculate the standard deviation
    let sum_of_squares: f64 = round_trip_times
        .iter()
        .map(|l| (l.as_micros() as f64 - avg_round_trip_time).powi(2))
        .sum();
    let variance = sum_of_squares / n_rtt as f64;
    let std_dev = variance.sqrt();
    println!(
        "Round trip time: {:.2} +/- {:.2} us",
        avg_round_trip_time, std_dev
    );

    // Estimate the throughput
    let total_bytes = n_rtt * PACKET_SIZE;
    let total_time: f64 = round_trip_times.iter().map(|l| l.as_secs_f64()).sum();
    let throughput = total_bytes as f64 / total_time;
    println!("Throughput: {} bytes/second", throughput as u64);
    println!(
        "Throughput: {} msgs/second",
        (throughput / PACKET_SIZE as f64) as u64
    );
}

fn generate_msg(
    seq_num: u8,
    ids: &[u32],
    definitions: &HashMap<u32, Offsets>,
    sender: (u8, u8),
    target: (u8, u8),
) -> (u32, [u8; PACKET_SIZE]) {
    let id = ids[rand::random::<usize>() % ids.len()];
    let offset = definitions.get(&id).unwrap();

    let mut msg = [0u8; PACKET_SIZE];
    rand::thread_rng().fill(&mut msg[..]);

    msg[0] = mavlink::v2::PACKET_MAGIC;
    msg[1] = 255; // Payload length
    msg[2] = 0; // Incompatibility flags
    msg[3] = 0; // Compatibility flags
    msg[4] = seq_num; // Sequence number
    msg[5] = sender.0; // Sender system ID
    msg[6] = sender.1; // Sender component ID
    let id_bytes = id.to_le_bytes();
    if id_bytes[3] != 0 {
        panic!("ID too large: {}", id);
    }
    msg[7] = id_bytes[0];
    msg[8] = id_bytes[1];
    msg[9] = id_bytes[2];

    let payload = &mut msg[mavlink::v2::HEADER_LEN..];

    payload[offset.system_id] = target.0;
    if let Some(comp_id) = offset.component_id {
        payload[comp_id] = target.1;
    }

    (id, msg)
}

struct Data {
    id: u32,
    seq_num: u8,
    send_time: Instant,
}

fn record_round_trip_times(
    tid: usize,
    ids: Arc<[u32]>,
    definitions: Arc<HashMap<u32, Offsets>>,
    config: Config,
) -> Result<Vec<Duration>> {
    const SYS_ID: u8 = 1;

    let sender_comp_id = 1 + 2 * tid as u8;
    let target_comp_id = 2 + 2 * tid as u8;

    let tgt_addr: SocketAddr = format!("127.0.0.1:{}", 14550 + 2 * tid).parse()?;
    let rcv_addr: SocketAddr = format!("127.0.0.1:{}", 14551 + 2 * tid).parse()?;

    let nr_messages = (config.duration.as_secs_f64() * config.frequency) as usize;

    // Create a bunch of messages
    let messages = (0..nr_messages)
        .map(|i| {
            generate_msg(
                i as u8,
                ids.as_ref(),
                definitions.as_ref(),
                (SYS_ID, sender_comp_id),
                (SYS_ID, target_comp_id),
            )
        })
        .collect::<Vec<_>>();

    // Create a udp socket to send messages
    let send_socket_addr = format!("127.0.0.1:{}", 14540 + 2 * tid);
    let recv_socket_addr = format!("127.0.0.1:{}", 14541 + 2 * tid);

    let tid = tid.to_string();

    info!(target: &tid, "Binding send to {}", send_socket_addr);
    let send_socket = UdpSocket::bind(send_socket_addr)?;
    info!(target: &tid, "Binding recv to {}", recv_socket_addr);
    let recv_socket = UdpSocket::bind(recv_socket_addr)?;

    // Send a message from the rcv socket to the target to establish a connection
    {
        let (_, msg) = generate_msg(
            0,
            ids.as_ref(),
            definitions.as_ref(),
            (SYS_ID, target_comp_id),
            (SYS_ID, sender_comp_id),
        );
        debug!(target: &tid, "Sending connection message to {}", rcv_addr);
        recv_socket.send_to(&msg, rcv_addr)?;
    }

    std::thread::sleep(Duration::from_millis(10));

    // Send the messages
    info!(target: &tid,
        "Sending {} messages to {}",
        messages.len(),
        tgt_addr
    );

    if config.same_thread {
        record_st(tid, send_socket, recv_socket, messages, tgt_addr)
    } else {
        record_mt(tid, send_socket, recv_socket, messages, tgt_addr, config)
    }
}

fn record_st(
    tid: String,
    send_socket: UdpSocket,
    recv_socket: UdpSocket,
    messages: Vec<(u32, [u8; PACKET_SIZE])>,
    tgt_addr: SocketAddr,
) -> Result<Vec<Duration>> {
    let mut round_trip_times = Vec::with_capacity(messages.len());

    let mut buf = [0u8; mavlink::v2::MAX_PACKET_LEN * 5];
    for (i, (id, msg)) in messages.iter().enumerate() {
        debug!(target: &tid, "Sending msg to {tgt_addr}: seq_num {}, id {id}", i as u8);
        let now = Instant::now();
        send_socket.send_to(msg, tgt_addr)?;
        let (len, recv_addr) = recv_socket.recv_from(&mut buf)?;
        let rtt = now.elapsed();

        if len < mavlink::v2::MIN_PACKET_LEN {
            bail!("Received message too short");
        }
        if len > PACKET_SIZE {
            bail!("Received message too long ({} > {})", len, PACKET_SIZE);
        }

        let msg = &buf[..len];
        let seq_num = msg[4];
        let msg_id = u32::from_le_bytes([msg[7], msg[8], msg[9], 0]);
        debug!(target: &tid, "Received msg from {recv_addr}: seq_num {seq_num}, id {id}, len {len}");
        if msg_id != *id {
            warn!(target: &tid, "Expected message id {id}, got {msg_id}");
        }
        if seq_num != i as u8 {
            warn!(target: &tid,
                "Expected sequence number {}, got {seq_num}",
                i as u8
            );
        }

        round_trip_times.push(rtt);
    }

    Ok(round_trip_times)
}

fn record_mt(
    tid: String,
    send_socket: UdpSocket,
    recv_socket: UdpSocket,
    messages: Vec<(u32, [u8; PACKET_SIZE])>,
    tgt_addr: SocketAddr,
    config: Config,
) -> Result<Vec<Duration>> {
    let (tx, rx) = mpsc::channel::<Data>();

    // Start the firehose
    let send_tid = tid.clone();
    let sender_handle = thread::spawn(move || {
        send_msgs(
            send_tid,
            send_socket,
            messages,
            tgt_addr,
            tx,
            config.frequency,
        )
    });
    let receiver_handle = thread::spawn(move || recv_msgs(tid, recv_socket, rx));

    sender_handle.join().unwrap()?;

    receiver_handle.join().unwrap()
}

fn send_msgs(
    tid: String,
    socket: UdpSocket,
    messages: Vec<(u32, [u8; PACKET_SIZE])>,
    tgt_addr: SocketAddr,
    tx: mpsc::Sender<Data>,
    frequency: f64,
) -> Result<()> {
    let sample_time: Duration = Duration::from_secs_f64(1.0 / frequency);

    for (i, (id, msg)) in messages.iter().enumerate() {
        let now = Instant::now();
        socket.send_to(msg, tgt_addr)?;
        let data = Data {
            id: *id,
            seq_num: i as u8,
            send_time: Instant::now(),
        };
        tx.send(data)?;

        if i % frequency as usize == 0 {
            info!(target: &tid, "Sent {} msgs", i);
        }
        let sleep_dur = sample_time.saturating_sub(now.elapsed());
        if sleep_dur > Duration::ZERO {
            thread::sleep(sleep_dur);
        }
    }
    Ok(())
}

fn recv_msgs(tid: String, socket: UdpSocket, rx: mpsc::Receiver<Data>) -> Result<Vec<Duration>> {
    let mut round_trip_times = Vec::with_capacity(NUM_MESSAGES);

    let mut buf = [0u8; mavlink::v2::MAX_PACKET_LEN * 5];
    while let Ok(data) = rx.recv() {
        let rtt = recv_msg(&tid, &socket, &mut buf, data)?;
        round_trip_times.push(rtt);
    }

    Ok(round_trip_times)
}

fn recv_msg(tid: &str, socket: &UdpSocket, buf: &mut [u8], data: Data) -> Result<Duration> {
    let (len, recv_addr) = socket.recv_from(buf)?;
    let recv_time = Instant::now();
    if len < mavlink::v2::MIN_PACKET_LEN {
        bail!("Received message too short");
    }
    if len > PACKET_SIZE {
        bail!("Received message too long ({} > {})", len, PACKET_SIZE);
    }

    let msg = &buf[..len];
    let seq_num = msg[4];
    let msg_id = u32::from_le_bytes([msg[7], msg[8], msg[9], 0]);
    debug!(target: tid,
        "Received msg from {}: seq_num {}, id {}, len {}",
        recv_addr, seq_num, msg_id, len
    );
    if msg_id != data.id {
        warn!(target: tid,
            "Expected message id {}, got {}",
            data.id, msg_id
        );
    }
    if seq_num != data.seq_num {
        warn!(target: tid,
            "Expected sequence number {}, got {}",
            data.seq_num, seq_num
        );
    }

    Ok(recv_time - data.send_time)
}
