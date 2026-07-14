use crate::audio;
use crate::models::PeerState;
use crate::network;
use cpal::traits::{DeviceTrait, StreamTrait};
use opus::{Application, Channels, Decoder, Encoder};
use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
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

pub fn start_voice_engine(
    username: String,
    room: String,
    selected_input_name: String,
    selected_output_name: String,
    volume_level_ref: Arc<Mutex<f32>>,
    status: Arc<Mutex<String>>,
    kill_signal: Arc<AtomicBool>,
    is_muted: Arc<AtomicBool>,
    is_deafened: Arc<AtomicBool>,
    active_peers: Arc<Mutex<HashMap<SocketAddr, PeerState>>>,
) {
    std::thread::spawn(move || {
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();

        *status.lock().unwrap() = "Получаю IP (STUN)...".to_string();
        let my_public_addr = match network::get_public_ip(&socket) {
            Some(addr) => addr,
            None => {
                *status.lock().unwrap() = "Ошибка сети (STUN)".to_string();
                kill_signal.store(true, Ordering::Relaxed);
                return;
            }
        };

        let socket_receiver = socket.try_clone().expect("Не удалось клонировать сокет");
        let socket_sender = socket.try_clone().expect("Не удалось клонировать сокет");
        let socket_sender_pong = socket.try_clone().expect("Не удалось клонировать сокет");
        let socket_sender_ping = socket.try_clone().expect("Не удалось клонировать сокет");

        *status.lock().unwrap() = "В комнате".to_string();

        let ws_url = format!("wss://p2p.tallfly.me/ws/{}", room);
        let (mut ws_socket, _) = match connect(&ws_url) {
            Ok(s) => s,
            Err(e) => {
                *status.lock().unwrap() = format!("Ошибка WS: {}", e);
                return;
            }
        };

        let my_info = format!("{}|{}", my_public_addr, username);
        ws_socket.send(Message::Text(my_info.clone())).unwrap();

        let peers_ws = active_peers.clone();
        let socket_puncher = socket.try_clone().unwrap();
        let my_info_clone = my_info.clone();
        let kill_signal_ws = kill_signal.clone();

        std::thread::spawn(move || {
            while !kill_signal_ws.load(Ordering::Relaxed) {
                match ws_socket.read() {
                    Ok(Message::Text(text)) => {
                        if text != my_info_clone {
                            if let Some((ip_str, name)) = text.split_once('|') {
                                if let Ok(peer_addr) = ip_str.parse::<SocketAddr>() {
                                    let mut p = peers_ws.lock().unwrap();
                                    if !p.contains_key(&peer_addr) {
                                        p.insert(
                                            peer_addr,
                                            PeerState {
                                                name: name.to_string(),
                                                last_seen: Instant::now(),
                                                last_spoken: Instant::now()
                                                    - Duration::from_secs(10),
                                                volume: 1.0,
                                                ping_ms: 0,
                                            },
                                        );
                                        for _ in 0..5 {
                                            let _ =
                                                socket_puncher.send_to(b"HOLE_PUNCH", &peer_addr);
                                        }
                                        let _ =
                                            ws_socket.send(Message::Text(my_info_clone.clone()));
                                    }
                                }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            let _ = ws_socket.close(None);
        });

        let host = cpal::default_host();

        let input_device = match audio::find_device_by_name(&host, &selected_input_name, true) {
            Some(d) => d,
            None => {
                *status.lock().unwrap() = "Микрофон не найден".to_string();
                return;
            }
        };

        let output_device = match audio::find_device_by_name(&host, &selected_output_name, false) {
            Some(d) => d,
            None => {
                *status.lock().unwrap() = "Динамики не найдены".to_string();
                return;
            }
        };

        let input_config = match input_device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                *status.lock().unwrap() = format!("Ошибка конфига микрофона: {}", e);
                return;
            }
        };

        let output_config = match output_device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                *status.lock().unwrap() = format!("Ошибка конфига динамиков: {}", e);
                return;
            }
        };

        let input_channels = input_config.channels() as usize;
        let input_stream_config = input_config.config();
        let hardware_sample_rate = output_config.config().sample_rate.0 as f32;
        let hardware_channels = output_config.config().channels as usize;
        let output_stream_config = output_config.config();
        let resample_ratio = 48000.0 / hardware_sample_rate;

        let peers_audio = Arc::new(Mutex::new(HashMap::<SocketAddr, PeerAudio>::new()));
        let source_idx_map = Arc::new(Mutex::new(HashMap::<SocketAddr, f32>::new()));

        socket
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();

        let kill_signal_rx = kill_signal.clone();
        let active_peers_rx = active_peers.clone();
        let peers_audio_rx = peers_audio.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];

            while !kill_signal_rx.load(Ordering::Relaxed) {
                match socket_receiver.recv_from(&mut buf) {
                    Ok((amt, src)) => {
                        let now = Instant::now();

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
                            let _ = socket_sender_pong.send_to(&pong, src);
                            continue;
                        }

                        if amt == 12 && &buf[..4] == b"PONG" {
                            let mut ts_bytes = [0u8; 8];
                            ts_bytes.copy_from_slice(&buf[4..12]);
                            let sent_time = u64::from_be_bytes(ts_bytes);
                            let ping = current_time_ms().saturating_sub(sent_time) as u32;
                            active_peers_rx.lock().unwrap().entry(src).and_modify(|s| {
                                s.last_seen = now;
                                s.ping_ms = ping;
                            });
                            continue;
                        }

                        if amt < 2 || amt == 4 || amt == 10 || amt == 12 {
                            continue;
                        }

                        active_peers_rx.lock().unwrap().entry(src).and_modify(|s| {
                            s.last_seen = now;
                            s.last_spoken = now;
                        });

                        let seq = u16::from_be_bytes([buf[0], buf[1]]);
                        let payload = buf[2..amt].to_vec();

                        let mut pa_map = peers_audio_rx.lock().unwrap();
                        let pa = pa_map.entry(src).or_insert_with(PeerAudio::default);

                        if let Some(expected) = pa.next_seq {
                            if (seq.wrapping_sub(expected) as i16) < 0 {
                                continue;
                            }
                        }

                        pa.jb_packets.insert(seq, payload);

                        if pa.jb_packets.len() > 10 {
                            let first_key = *pa.jb_packets.keys().next().unwrap();
                            pa.jb_packets.remove(&first_key);
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });

        let mut encoder = Encoder::new(48000, Channels::Mono, Application::Voip).unwrap();
        let mut input_buffer = Vec::new();
        let active_peers_tx = active_peers.clone();
        let is_deafened_tx = is_deafened.clone();
        let is_muted_tx = is_muted.clone();

        let mut seq_num: u16 = 0;
        let mut hangover_frames = 0;
        const HANGOVER_THRESHOLD: usize = 15;

        let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut peak = 0.0_f32;
            for frame in data.chunks(input_channels) {
                input_buffer.push(frame[0]);
                if frame[0].abs() > peak {
                    peak = frame[0].abs();
                }
            }
            *volume_level_ref.lock().unwrap() = (peak * 5.0).clamp(0.0, 1.0);

            if peak > 0.015 {
                hangover_frames = HANGOVER_THRESHOLD;
            }

            let is_speaking = !is_muted_tx.load(Ordering::Relaxed)
                && !is_deafened_tx.load(Ordering::Relaxed)
                && hangover_frames > 0;

            if is_speaking {
                if peak <= 0.015 && hangover_frames > 0 {
                    hangover_frames -= 1;
                }

                while input_buffer.len() >= 960 {
                    let mut chunk = [0f32; 960];
                    chunk.copy_from_slice(&input_buffer[0..960]);
                    input_buffer.drain(0..960);

                    let mut out_bytes = [0u8; 1022];
                    out_bytes[0..2].copy_from_slice(&seq_num.to_be_bytes());

                    if let Ok(size) = encoder.encode_float(&chunk, &mut out_bytes[2..]) {
                        let packet_size = size + 2;
                        let peers = active_peers_tx.lock().unwrap();
                        for (peer, _) in peers.iter() {
                            let _ = socket_sender.send_to(&out_bytes[..packet_size], peer);
                        }
                        seq_num = seq_num.wrapping_add(1);
                    }
                }
            } else {
                input_buffer.clear();
            }
        };

        let peers_audio_tx = peers_audio.clone();
        let source_idx_tx = source_idx_map.clone();
        let active_peers_out = active_peers.clone();
        let is_deafened_out = is_deafened.clone();

        let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if is_deafened_out.load(Ordering::Relaxed) {
                for ch in data.iter_mut() {
                    *ch = 0.0;
                }
                return;
            }

            let volumes: HashMap<SocketAddr, f32> = active_peers_out
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (*k, v.volume))
                .collect();

            let mut audio_states = peers_audio_tx.lock().unwrap();
            let mut indices = source_idx_tx.lock().unwrap();

            for (_addr, pa) in audio_states.iter_mut() {
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

                            if let Ok(count) = pa.decoder.decode_float(&packet, &mut decoded, false)
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

                                if let Ok(count) = pa.decoder.decode_float(&[], &mut decoded, false)
                                {
                                    pa.pcm_queue.extend(&decoded[..count]);
                                }
                                pa.next_seq = Some(expected.wrapping_add(1));
                            }
                        }
                    }
                }
            }

            for frame in data.chunks_mut(hardware_channels) {
                let mut mixed = 0.0_f32;
                for (addr, pa) in audio_states.iter_mut() {
                    let vol = volumes.get(addr).unwrap_or(&1.0);
                    let idx_ref = indices.entry(*addr).or_insert(0.0);
                    let i = *idx_ref as usize;

                    let mut sample = 0.0_f32;
                    if pa.pcm_queue.len() > i + 1 {
                        sample = pa.pcm_queue[i]
                            + (*idx_ref - i as f32) * (pa.pcm_queue[i + 1] - pa.pcm_queue[i]);
                    } else if let Some(&s) = pa.pcm_queue.get(i) {
                        sample = s;
                    }

                    mixed += sample * vol;

                    *idx_ref += resample_ratio;
                    if *idx_ref >= 1.0 {
                        let advance = idx_ref.floor() as usize;
                        for _ in 0..advance {
                            pa.pcm_queue.pop_front();
                        }
                        *idx_ref -= advance as f32;
                    }
                }

                mixed = mixed.clamp(-1.0, 1.0);
                for ch in frame.iter_mut() {
                    *ch = mixed;
                }
            }
        };

        let input_stream = match input_device.build_input_stream(
            &input_stream_config,
            input_data_fn,
            audio::err_fn,
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                *status.lock().unwrap() = format!("Не удалось запустить микрофон: {}", e);
                return;
            }
        };
        let output_stream = match output_device.build_output_stream(
            &output_stream_config,
            output_data_fn,
            audio::err_fn,
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                *status.lock().unwrap() = format!("Не удалось запустить динамики: {}", e);
                return;
            }
        };
        input_stream.play().unwrap();
        output_stream.play().unwrap();

        while !kill_signal.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
            let now = Instant::now();
            let mut ping_packet = [0u8; 12];
            ping_packet[..4].copy_from_slice(b"PING");
            ping_packet[4..12].copy_from_slice(&current_time_ms().to_be_bytes());

            active_peers.lock().unwrap().retain(|addr, state| {
                if now.duration_since(state.last_seen).as_secs() > 5 {
                    peers_audio.lock().unwrap().remove(addr);
                    source_idx_map.lock().unwrap().remove(addr);
                    false
                } else {
                    let _ = socket_sender_ping.send_to(&ping_packet, addr);
                    true
                }
            });
        }
    });
}
