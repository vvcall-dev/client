use crate::audio;
use crate::models::PeerState;
use crate::network;
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender, unbounded};
use opus::{Application, Channels, Decoder, Encoder};
use rand::Rng;
use ringbuf::{
    HeapRb,
    traits::{Consumer, Producer, Split},
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tungstenite::{Message, connect};

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

pub struct EngineArgs {
    pub server_url: String,
    pub username: String,
    pub room: String,
    pub room_password: String,
    pub selected_input: String,
    pub selected_output: String,
    pub volume_level: Arc<Mutex<f32>>,
    pub status: Arc<Mutex<String>>,
    pub kill_signal: Arc<AtomicBool>,
    pub is_muted: Arc<AtomicBool>,
    pub is_deafened: Arc<AtomicBool>,
    pub active_peers: Arc<Mutex<HashMap<SocketAddr, PeerState>>>,
}

struct NetPacket {
    src: SocketAddr,
    seq: u16,
    payload: Vec<u8>,
}

pub fn start_voice_engine(args: EngineArgs) {
    std::thread::spawn(move || {
        *args.status.lock().unwrap() = "Инициализация движка...".to_string();

        let mic_rb = HeapRb::<f32>::new(48000 * 2);
        let (mic_prod, mic_cons) = mic_rb.split();

        let spk_rb = HeapRb::<f32>::new(48000 * 2);
        let (spk_prod, spk_cons) = spk_rb.split();

        let (tx_net_out, rx_net_out) = unbounded::<Vec<u8>>();
        let (tx_net_in, rx_net_in) = unbounded::<NetPacket>();

        let hardware_streams = spawn_hardware(&args, mic_prod, spk_cons);
        if hardware_streams.is_none() {
            return;
        }

        spawn_audio_processor(&args, mic_cons, spk_prod, tx_net_out, rx_net_in);
        spawn_network(&args, rx_net_out, tx_net_in);

        while !args.kill_signal.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

fn spawn_hardware(
    args: &EngineArgs,
    mut mic_prod: impl Producer<Item = f32> + Send + 'static,
    mut spk_cons: impl Consumer<Item = f32> + Send + 'static,
) -> Option<(cpal::Stream, cpal::Stream)> {
    let host = cpal::default_host();
    let input_device = audio::find_device_by_name(&host, &args.selected_input, true)?;
    let output_device = audio::find_device_by_name(&host, &args.selected_output, false)?;

    let input_config = input_device.default_input_config().unwrap();
    let output_config = output_device.default_output_config().unwrap();
    let in_channels = input_config.channels() as usize;
    let out_channels = output_config.channels() as usize;

    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        for frame in data.chunks(in_channels) {
            let _ = mic_prod.try_push(frame[0]);
        }
    };

    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        for frame in data.chunks_mut(out_channels) {
            let sample = spk_cons.try_pop().unwrap_or(0.0);
            for ch in frame.iter_mut() {
                *ch = sample;
            }
        }
    };

    let in_stream = input_device
        .build_input_stream(&input_config.config(), input_data_fn, audio::err_fn, None)
        .unwrap();
    let out_stream = output_device
        .build_output_stream(&output_config.config(), output_data_fn, audio::err_fn, None)
        .unwrap();

    in_stream.play().unwrap();
    out_stream.play().unwrap();

    Some((in_stream, out_stream))
}

struct PeerAudio {
    decoder: Decoder,
    jb_packets: std::collections::BTreeMap<u16, Vec<u8>>,
    pcm_queue: VecDeque<f32>,
    next_seq: Option<u16>,
    buffering: bool,
    target_buffer: usize,
    good_frames: usize,
}

impl Default for PeerAudio {
    fn default() -> Self {
        Self {
            decoder: Decoder::new(48000, Channels::Mono).unwrap(),
            jb_packets: std::collections::BTreeMap::new(),
            pcm_queue: VecDeque::with_capacity(2000),
            next_seq: None,
            buffering: true,
            target_buffer: 1,
            good_frames: 0,
        }
    }
}

fn spawn_audio_processor(
    args: &EngineArgs,
    mut mic_cons: impl Consumer<Item = f32> + Send + 'static,
    mut spk_prod: impl Producer<Item = f32> + Send + 'static,
    tx_net_out: Sender<Vec<u8>>,
    rx_net_in: Receiver<NetPacket>,
) {
    let mut encoder = Encoder::new(48000, Channels::Mono, Application::Voip).unwrap();
    let mut input_buffer = Vec::with_capacity(960);
    let mut audio_states: HashMap<SocketAddr, PeerAudio> = HashMap::new();

    let is_muted = args.is_muted.clone();
    let is_deafened = args.is_deafened.clone();
    let vol_ref = args.volume_level.clone();
    let kill_signal = args.kill_signal.clone();
    let active_peers = args.active_peers.clone();

    std::thread::spawn(move || {
        let mut hangover_frames = 0;
        let mut cleanup_counter = 0;

        while !kill_signal.load(Ordering::Relaxed) {
            while let Some(sample) = mic_cons.try_pop() {
                input_buffer.push(sample);
            }

            while input_buffer.len() >= 960 {
                let mut chunk = [0f32; 960];
                chunk.copy_from_slice(&input_buffer[0..960]);
                input_buffer.drain(0..960);

                let peak = chunk.iter().fold(0.0_f32, |max, &val| max.max(val.abs()));
                *vol_ref.lock().unwrap() = (peak * 5.0).clamp(0.0, 1.0);

                if peak > 0.015 {
                    hangover_frames = 15;
                }

                let is_speaking = !is_muted.load(Ordering::Relaxed)
                    && !is_deafened.load(Ordering::Relaxed)
                    && hangover_frames > 0;

                if is_speaking {
                    if peak <= 0.015 {
                        hangover_frames -= 1;
                    }
                    let mut opus_buf = [0u8; 1000];
                    if let Ok(size) = encoder.encode_float(&chunk, &mut opus_buf) {
                        let _ = tx_net_out.send(opus_buf[..size].to_vec());
                    }
                }
            }

            while let Ok(packet) = rx_net_in.try_recv() {
                let pa = audio_states
                    .entry(packet.src)
                    .or_insert_with(PeerAudio::default);

                if let Some(expected) = pa.next_seq {
                    if (packet.seq.wrapping_sub(expected) as i16) < 0 {
                        continue;
                    }
                }
                pa.jb_packets.insert(packet.seq, packet.payload);
                if pa.jb_packets.len() > 10 {
                    let first_key = *pa.jb_packets.keys().next().unwrap();
                    pa.jb_packets.remove(&first_key);
                }
            }

            if spk_prod.vacant_len() >= 960 {
                let active_keys: Vec<SocketAddr> = {
                    let peers = active_peers.lock().unwrap();
                    peers.keys().copied().collect()
                };

                let volumes: HashMap<SocketAddr, f32> = {
                    let peers = active_peers.lock().unwrap();
                    peers.iter().map(|(k, v)| (*k, v.volume)).collect()
                };

                cleanup_counter += 1;
                if cleanup_counter >= 500 {
                    audio_states.retain(|addr, _| active_keys.contains(addr));
                    cleanup_counter = 0;
                }

                while spk_prod.vacant_len() >= 960 {
                    if is_deafened.load(Ordering::Relaxed) {
                        let silence = [0.0f32; 960];
                        let _ = spk_prod.push_slice(&silence);
                        continue;
                    }

                    let mut mixed = [0f32; 960];

                    for (addr, pa) in audio_states.iter_mut() {
                        while pa.pcm_queue.len() < 960 {
                            if pa.buffering {
                                if pa.jb_packets.len() >= pa.target_buffer {
                                    pa.buffering = false;
                                    pa.next_seq = Some(*pa.jb_packets.keys().next().unwrap());
                                } else {
                                    break;
                                }
                            }

                            if !pa.buffering {
                                let expected = pa.next_seq.unwrap();
                                let mut decoded = [0f32; 960];

                                if let Some(packet) = pa.jb_packets.remove(&expected) {
                                    pa.good_frames += 1;
                                    if pa.good_frames > 250 && pa.target_buffer > 1 {
                                        pa.target_buffer -= 1;
                                        pa.good_frames = 0;
                                    }
                                    if let Ok(count) =
                                        pa.decoder.decode_float(&packet, &mut decoded, false)
                                    {
                                        pa.pcm_queue.extend(&decoded[..count]);
                                    }
                                    pa.next_seq = Some(expected.wrapping_add(1));
                                } else {
                                    pa.good_frames = 0;
                                    if pa.jb_packets.is_empty() {
                                        pa.buffering = true;
                                        break;
                                    } else {
                                        if pa.target_buffer < 5 {
                                            pa.target_buffer += 1;
                                        }
                                        if let Ok(count) =
                                            pa.decoder.decode_float(&[], &mut decoded, false)
                                        {
                                            pa.pcm_queue.extend(&decoded[..count]);
                                        }
                                        pa.next_seq = Some(expected.wrapping_add(1));
                                    }
                                }
                            }
                        }

                        let vol = volumes.get(addr).unwrap_or(&1.0);
                        if pa.pcm_queue.len() >= 960 {
                            for i in 0..960 {
                                mixed[i] += pa.pcm_queue.pop_front().unwrap() * vol;
                            }
                        }
                    }

                    for sample in mixed.iter_mut() {
                        *sample = (*sample).tanh();
                    }
                    let _ = spk_prod.push_slice(&mixed);
                }
            }

            std::thread::sleep(Duration::from_millis(2));
        }
    });
}

fn spawn_network(args: &EngineArgs, rx_net_out: Receiver<Vec<u8>>, tx_net_in: Sender<NetPacket>) {
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();

    let mut room_hasher = Sha256::new();
    room_hasher.update(format!("{}:{}", args.room, args.room_password).as_bytes());
    let secure_room_hash = hex::encode(room_hasher.finalize());

    let mut key_hasher = Sha256::new();
    key_hasher.update(args.room_password.as_bytes());
    key_hasher.update(b"tallfly_p2p_salt");
    let aes_key_bytes = key_hasher.finalize();
    let aes_key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes).clone();

    let my_peer_id: u32 = rand::random();

    let host = args
        .server_url
        .split(':')
        .next()
        .unwrap_or(&args.server_url);
    let relay_addr: SocketAddr = format!("{}:3031", host)
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    *args.status.lock().unwrap() = "Получаю IP (STUN)...".to_string();
    let my_public_addr = match network::get_public_ip(&socket) {
        Some(addr) => addr,
        None => {
            *args.status.lock().unwrap() = "Ошибка сети (STUN)".to_string();
            args.kill_signal.store(true, Ordering::Relaxed);
            return;
        }
    };

    let secure_room_hash_rx = secure_room_hash.clone();

    let mut relay_header = Vec::with_capacity(68);
    relay_header.extend_from_slice(secure_room_hash.as_bytes());
    relay_header.extend_from_slice(&my_peer_id.to_be_bytes());

    let socket_rx = socket.try_clone().unwrap();
    let socket_tx = socket.try_clone().unwrap();
    let socket_ping = socket.try_clone().unwrap();
    let socket_puncher = socket.try_clone().unwrap();
    let socket_pong = socket.try_clone().unwrap();

    let direct_rx_map = Arc::new(Mutex::new(HashMap::<SocketAddr, Instant>::new()));
    let peer_id_map = Arc::new(Mutex::new(HashMap::<u32, SocketAddr>::new()));

    *args.status.lock().unwrap() = "В комнате".to_string();

    let scheme = if args.server_url.contains("localhost") || args.server_url.contains("127.0.0.1") {
        "http"
    } else {
        "https"
    };
    let ws_scheme = if scheme == "http" { "ws" } else { "wss" };
    let ws_url = format!(
        "{}://{}/ws/{}",
        ws_scheme, args.server_url, secure_room_hash
    );

    let (mut ws_socket, _) = connect(&ws_url).expect("Ошибка WebSocket");
    let my_info = format!("{}|{}|{}", my_public_addr, args.username, my_peer_id);
    ws_socket.send(Message::Text(my_info.clone())).unwrap();

    let peers_ws = args.active_peers.clone();
    let peer_id_map_ws = peer_id_map.clone();
    let my_info_clone = my_info.clone();
    let kill_signal_ws = args.kill_signal.clone();

    std::thread::spawn(move || {
        while !kill_signal_ws.load(Ordering::Relaxed) {
            if let Ok(Message::Text(text)) = ws_socket.read() {
                if text != my_info_clone {
                    let parts: Vec<&str> = text.split('|').collect();
                    if parts.len() == 3 {
                        if let (Ok(addr), Ok(id)) =
                            (parts[0].parse::<SocketAddr>(), parts[2].parse::<u32>())
                        {
                            peer_id_map_ws.lock().unwrap().insert(id, addr);
                            let mut p = peers_ws.lock().unwrap();
                            if !p.contains_key(&addr) {
                                p.insert(
                                    addr,
                                    PeerState {
                                        name: parts[1].to_string(),
                                        last_seen: Instant::now(),
                                        last_spoken: Instant::now() - Duration::from_secs(10),
                                        volume: 1.0,
                                        ping_ms: 0,
                                    },
                                );
                                for _ in 0..5 {
                                    let _ = socket_puncher.send_to(b"HOLE_PUNCH", addr);
                                }
                                let _ = ws_socket.send(Message::Text(my_info_clone.clone()));
                            }
                        }
                    }
                }
            }
        }
        let _ = ws_socket.close(None);
    });

    let kill_signal_rx = args.kill_signal.clone();
    let direct_rx_map_rx = direct_rx_map.clone();
    let peer_id_map_rx = peer_id_map.clone();
    let active_peers_rx = args.active_peers.clone();
    let cipher_rx = Aes256Gcm::new(&aes_key);

    std::thread::spawn(move || {
        let mut buf = [0u8; 2048];
        socket_rx
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();

        while !kill_signal_rx.load(Ordering::Relaxed) {
            match socket_rx.recv_from(&mut buf) {
                Ok((mut amt, mut src)) => {
                    let now = Instant::now();

                    if src == relay_addr {
                        if amt > 68 {
                            let hash_str = String::from_utf8_lossy(&buf[..64]);
                            if hash_str == secure_room_hash_rx {
                                let peer_id =
                                    u32::from_be_bytes([buf[64], buf[65], buf[66], buf[67]]);
                                if let Some(&addr) = peer_id_map_rx.lock().unwrap().get(&peer_id) {
                                    src = addr;
                                    buf.copy_within(68..amt, 0);
                                    amt -= 68;
                                } else {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else {
                        direct_rx_map_rx.lock().unwrap().insert(src, now);
                    }

                    if amt == 10 && &buf[..10] == b"HOLE_PUNCH" {
                        active_peers_rx
                            .lock()
                            .unwrap()
                            .entry(src)
                            .and_modify(|s| s.last_seen = now);
                        continue;
                    }
                    if amt == 12 && &buf[..4] == b"PING" {
                        active_peers_rx
                            .lock()
                            .unwrap()
                            .entry(src)
                            .and_modify(|s| s.last_seen = now);
                        let mut pong = [0u8; 12];
                        pong[..4].copy_from_slice(b"PONG");
                        pong[4..12].copy_from_slice(&buf[4..12]);
                        let _ = socket_pong.send_to(&pong, src);
                        continue;
                    }
                    if amt == 12 && &buf[..4] == b"PONG" {
                        let mut ts_bytes = [0u8; 8];
                        ts_bytes.copy_from_slice(&buf[4..12]);
                        let sent_time = u64::from_be_bytes(ts_bytes);
                        active_peers_rx.lock().unwrap().entry(src).and_modify(|s| {
                            s.last_seen = now;
                            s.ping_ms = current_time_ms().saturating_sub(sent_time) as u32;
                        });
                        continue;
                    }
                    if amt < 30 {
                        continue;
                    }

                    let seq = u16::from_be_bytes([buf[0], buf[1]]);
                    let nonce = Nonce::from_slice(&buf[2..14]);

                    if let Ok(payload) = cipher_rx.decrypt(nonce, &buf[14..amt]) {
                        active_peers_rx.lock().unwrap().entry(src).and_modify(|s| {
                            s.last_seen = now;
                            s.last_spoken = now;
                        });
                        let _ = tx_net_in.send(NetPacket { src, seq, payload });
                    }
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::ConnectionReset {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
    });

    let active_peers_tx = args.active_peers.clone();
    let direct_rx_map_tx = direct_rx_map.clone();
    let cipher_tx = Aes256Gcm::new(&aes_key);
    let kill_signal_tx = args.kill_signal.clone();
    let relay_header_tx = relay_header.clone();

    std::thread::spawn(move || {
        let mut seq_num: u16 = 0;

        while !kill_signal_tx.load(Ordering::Relaxed) {
            if let Ok(opus_payload) = rx_net_out.recv_timeout(Duration::from_millis(100)) {
                let mut nonce_bytes = [0u8; 12];
                rand::thread_rng().fill(&mut nonce_bytes);
                let nonce = Nonce::from_slice(&nonce_bytes);

                if let Ok(ciphertext) = cipher_tx.encrypt(nonce, opus_payload.as_slice()) {
                    let mut final_packet = Vec::with_capacity(2 + 12 + ciphertext.len());
                    final_packet.extend_from_slice(&seq_num.to_be_bytes());
                    final_packet.extend_from_slice(&nonce_bytes);
                    final_packet.extend_from_slice(&ciphertext);

                    let peers = active_peers_tx.lock().unwrap();
                    let direct_rx = direct_rx_map_tx.lock().unwrap();
                    let now = Instant::now();

                    for (peer, _) in peers.iter() {
                        let last_direct = direct_rx
                            .get(peer)
                            .cloned()
                            .unwrap_or(now - Duration::from_secs(10));
                        if now.duration_since(last_direct).as_secs() > 2 {
                            let mut relay_packet = Vec::with_capacity(68 + final_packet.len());
                            relay_packet.extend_from_slice(&relay_header_tx);
                            relay_packet.extend_from_slice(&final_packet);
                            let _ = socket_tx.send_to(&relay_packet, relay_addr);
                        } else {
                            let _ = socket_tx.send_to(&final_packet, peer);
                        }
                    }
                    seq_num = seq_num.wrapping_add(1);
                }
            }
        }
    });

    let kill_signal_ping = args.kill_signal.clone();
    let active_peers_ping = args.active_peers.clone();
    let relay_header_ping = relay_header.clone();

    std::thread::spawn(move || {
        while !kill_signal_ping.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
            let now = Instant::now();
            let mut ping_packet = [0u8; 12];
            ping_packet[..4].copy_from_slice(b"PING");
            ping_packet[4..12].copy_from_slice(&current_time_ms().to_be_bytes());

            let mut peers = active_peers_ping.lock().unwrap();
            let mut direct_rx = direct_rx_map.lock().unwrap();

            direct_rx.retain(|_, last_seen| now.duration_since(*last_seen).as_secs() < 15);

            peers.retain(|addr, state| {
                if now.duration_since(state.last_seen).as_secs() > 5 {
                    false
                } else {
                    let _ = socket_ping.send_to(&ping_packet, addr);
                    let last_direct = direct_rx
                        .get(addr)
                        .cloned()
                        .unwrap_or(now - Duration::from_secs(10));
                    if now.duration_since(last_direct).as_secs() > 2 {
                        let mut relay_ping = Vec::with_capacity(68 + 12);
                        relay_ping.extend_from_slice(&relay_header_ping);
                        relay_ping.extend_from_slice(&ping_packet);
                        let _ = socket_ping.send_to(&relay_ping, relay_addr);
                    }
                    true
                }
            });
        }
    });
}
