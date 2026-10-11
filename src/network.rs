// use std::net::{Ipv4Addr, UdpSocket};
// use std::time::Duration;

// pub fn get_public_ip(socket: &UdpSocket) -> Option<String> {
//     socket.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
//
//     let stun_req: [u8; 20] = [
//         0, 1, 0, 0, 33, 18, 164, 66, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
//     ];
//
//     let servers = [
//         "stun.l.google.com:19302",
//         "stun1.l.google.com:19302",
//         "stun2.l.google.com:19302",
//         "stun3.l.google.com:19302",
//         "stun.cloudflare.com:3478",
//         "stun.stunprotocol.org:3478",
//     ];
//
//     for server in servers.iter() {
//         println!("STUN: Пытаюсь подключиться к {}", server);
//         if socket.send_to(&stun_req, server).is_ok() {
//             let mut buf = [0u8; 512];
//             if let Ok((len, _)) = socket.recv_from(&mut buf) {
//                 if len < 20 {
//                     continue;
//                 }
//
//                 let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
//                 if msg_type != 0x0101 {
//                     continue;
//                 }
//
//                 let mut pos = 20;
//                 while pos + 4 <= len {
//                     let attr_type = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
//                     let attr_len = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]) as usize;
//
//                     if attr_type == 0x0020 && attr_len >= 8 {
//                         let family = buf[pos + 5];
//                         if family == 0x01 {
//                             let port = u16::from_be_bytes([buf[pos + 6], buf[pos + 7]]) ^ 0x2112;
//                             let ip = Ipv4Addr::new(
//                                 buf[pos + 8] ^ 0x21,
//                                 buf[pos + 9] ^ 0x12,
//                                 buf[pos + 10] ^ 0xa4,
//                                 buf[pos + 11] ^ 0x42,
//                             );
//                             println!("STUN: Успех! IP: {}:{}", ip, port);
//                             return Some(format!("{}:{}", ip, port));
//                         }
//                     }
//                     pos += 4 + attr_len;
//                     pos = (pos + 3) & !3;
//                 }
//             }
//         } else {
//             println!("STUN: Ошибка отправки на {}", server);
//         }
//     }
//     println!("STUN: Не удалось получить IP ни от одного сервера");
//     None
// }
