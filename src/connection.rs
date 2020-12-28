use crate::ftl_codec::{FtlCodec, FtlCommand};
use futures::{SinkExt, StreamExt};
use hex::{decode, encode};
use rand::distributions::Uniform;
use rand::{thread_rng, Rng};
use ring::hmac;
use tokio::net::TcpStream;
use tokio::sync::{ mpsc};
use tokio_util::codec::Framed;

#[derive(Debug)]
enum FrameCommand {
    Send { data: Vec<String> },
    Kill,
}
pub struct Connection {}
#[derive(Debug)]
pub struct ConnectionState {
    pub hmac_payload: Option<String>,
    pub protocol_version: Option<String>,
    pub vendor_name: Option<String>,
    pub vendor_version: Option<String>,
    pub video: bool,
    pub video_codec: Option<String>,
    pub video_height: Option<String>,
    pub video_width: Option<String>,
    pub video_payload_type: Option<String>,
    pub video_ingest_ssrc: Option<String>,
    pub audio: bool,
    pub audio_codec: Option<String>,
    pub audio_payload_type: Option<String>,
    pub audio_ingest_ssrc: Option<String>,
}

impl ConnectionState {
    pub fn get_payload(&self) -> String {
        match &self.hmac_payload {
            Some(payload) => return payload.clone(),
            None => return "".to_string(),
        }
    }
    pub fn new() -> ConnectionState {
        ConnectionState {
            hmac_payload: None,
            protocol_version: None,
            vendor_name: None,
            vendor_version: None,
            video: false,
            video_codec: None,
            video_height: None,
            video_width: None,
            video_payload_type: None,
            video_ingest_ssrc: None,
            audio: false,
            audio_codec: None,
            audio_ingest_ssrc: None,
            audio_payload_type: None,
        }
    }
}
impl Connection {
    pub fn init(stream: TcpStream) {
        let (frame_send, mut conn_receive) = mpsc::channel::<FtlCommand>(2);
        let (conn_send, mut frame_receive) = mpsc::channel::<FrameCommand>(2);
        
        tokio::spawn(async move {
            let mut frame = Framed::new(stream, FtlCodec::new());
            loop {
                match frame.next().await {
                    Some(Ok(command)) => {
                        match frame_send.send(command).await {
                            Ok(_) => {
                                let command = frame_receive.recv().await;
                                match handle_frame_command(command, &mut frame).await {
                                    Ok(_) => {}
                                    Err(e) => {
                                        println!(
                                            "There was an error handing frame command {:?}",
                                            e
                                        );
                                        return;
                                    }
                                };
                            }
                            Err(e) => {
                                println!(
                                    "There was an error sending the command to the connection Error: {:?}", e
                                );
                                return;
                            }
                        };
                    }
                    Some(Err(e)) => {
                        println!("There was an error {:?}", e);
                        return;
                    }
                    None => {
                        println!("There was a socket reading error");
                        return;
                    }
                };
            }
        });

        tokio::spawn(async move {
            let mut state = ConnectionState::new();
            loop {
                match conn_receive.recv().await {
                    Some(FtlCommand::Disconnect) => {
                        //TODO: Determine what needs to happen here
                    },
                    Some(FtlCommand::Dot) => {
                        let resp_string = "200 hi. Use UDP port 10170\n".to_string();
                        let mut resp = Vec::new();
                        resp.push(resp_string);
                        match conn_send.send(FrameCommand::Send { data: resp }).await {
                            Ok(_) => {}
                            Err(e) => {
                                println!("Error sending to frame task (From: Handle HMAC) {:?}", e);
                                return;
                            }
                        }
                        // match upd_send
                        //     .send(UdpCommand::Start {
                        //         data: "9112".to_string(),
                        //     })
                        //     .await
                        // {
                        //     Ok(_) => {
                        //         let resp_string = "200 hi. Use UDP port 10170\n".to_string();
                        //         let mut resp = Vec::new();
                        //         resp.push(resp_string);
                        //         match conn_send.send(FrameCommand::Send { data: resp }).await {
                        //             Ok(_) => {}
                        //             Err(e) => {
                        //                 println!(
                        //                     "Error sending to frame task (From: Handle HMAC) {:?}",
                        //                     e
                        //                 );
                        //                 return;
                        //             }
                        //         }
                        //     }
                        //     Err(e) => println!("Error starting udp task"),
                        // }
                    }
                    Some(command) => {
                        handle_command(command, &conn_send, &mut state).await;
                    }
                    None => {
                        println!("Nothing received from the frame");
                        return;
                    }
                }
            }
        });
    }
}

async fn handle_frame_command(
    command: Option<FrameCommand>,
    frame: &mut Framed<TcpStream, FtlCodec>,
) -> Result<(), String> {
    match command {
        Some(FrameCommand::Send { data }) => {
            let mut d: Vec<String> = data.clone();
            d.reverse();
            while d.len() != 0 {
                let item = d.pop().unwrap();
                match frame.send(item.clone()).await {
                    Ok(_) => {
                        println!("sent {:?}", item);
                    }
                    Err(e) => {
                        println!("There was an error {:?}", e);
                        return Err(format!("There was an error {:?}", e));
                    }
                }
            }

            return Ok(());
        }
        Some(FrameCommand::Kill) => {
            println!("TODO: Implement Kill command");
            return Ok(());
        }
        None => {
            println!("Error receiving command from conn");
            return Err("Error receiving command from conn".to_string());
        }
    };
}

async fn handle_command(
    command: FtlCommand,
    sender: &mpsc::Sender<FrameCommand>,
    conn: &mut ConnectionState,
) {
    let mut resp: Vec<String>;
    match command {
        FtlCommand::HMAC => {
            resp = Vec::new();
            println!("Handling HMAC Command");
            conn.hmac_payload = Some(generate_hmac());
            resp.push("200 ".to_string());
            resp.push(conn.get_payload());
            resp.push("\n".to_string());
            match sender.send(FrameCommand::Send { data: resp }).await {
                Ok(_) => {
                    return;
                }
                Err(e) => {
                    println!("Error sending to frame task (From: Handle HMAC) {:?}", e);
                    return;
                }
            }
        }
        FtlCommand::Connect { data } => {
            resp = Vec::new();
            println!("Handling Connect Command");
            match (data.get("stream_key"), data.get("channel_id")) {
                (Some(key), Some(_channel_id)) => {
                    let client_hash = hex::decode(key).expect("error with hash decode");
                    //TODO: Add a more elegant stream key system
                    let key =
                        hmac::Key::new(hmac::HMAC_SHA512, b"aBcDeFgHiJkLmNoPqRsTuVwXyZ123456");
                    match hmac::verify(
                        &key,
                        decode(conn.get_payload().into_bytes())
                            .expect("error with payload decode")
                            .as_slice(),
                        &client_hash.as_slice(),
                    ) {
                        Ok(_) => {
                            println!("Hashes equal!");
                            resp.push("200\n".to_string());
                            match sender.send(FrameCommand::Send { data: resp }).await {
                                Ok(_) => {
                                    return;
                                }
                                Err(e) => println!(
                                    "Error sending to frame task (From: Handle Connection) {:?}",
                                    e
                                ),
                            }
                        }
                        _ => {
                            println!("Hashes do not equal");
                            return;
                        }
                    };
                    //temp stream key aBcDeFgHiJkLmNoPqRsTuVwXyZ123456
                }

                (None, _) => {
                    println!("No stream key attached to connect command");
                    return;
                }
                (_, None) => {
                    println!("No channel id attached to connect command");
                    return;
                }
            }
        }
        FtlCommand::Attribute { data } => {
            resp = Vec::new();
            println!("Handling Attribute Command");
            match (data.get("key"), data.get("value")) {
                (Some(key), Some(value)) => {
                    // println!("Key: {:?}, value: {:?}", key, value);
                    match key.as_str() {
                        "ProtocolVersion" => conn.protocol_version = Some(value.to_string()),
                        "VendorName" => conn.vendor_name = Some(value.to_string()),
                        "VendorVersion" => conn.vendor_version = Some(value.to_string()),
                        "Video" => {
                            match value.as_str() {
                                "true" => conn.video = true,
                                "false" => conn.video = false,
                                _ => {
                                    println!("Invalid video value! Atrribute parse failed. Value was: {:?}", value);
                                    return;
                                }
                            }
                        }
                        "VideoCodec" => conn.video_codec = Some(value.to_string()),
                        "VideoHeight" => conn.video_height = Some(value.to_string()),
                        "VideoWidth" => conn.video_width = Some(value.to_string()),
                        "VideoPayloadType" => conn.video_payload_type = Some(value.to_string()),
                        "VideoIngestSSRC" => conn.video_ingest_ssrc = Some(value.to_string()),
                        "Audio" => {
                            match value.as_str() {
                                "true" => conn.audio = true,
                                "false" => conn.audio = false,
                                _ => {
                                    println!("Invalid audio value! Atrribute parse failed. Value was: {:?}", value);
                                    return;
                                }
                            }
                        }
                        "AudioCodec" => conn.audio_codec = Some(value.to_string()),
                        "AudioPayloadType" => conn.audio_payload_type = Some(value.to_string()),
                        "AudioIngestSSRC" => conn.audio_ingest_ssrc = Some(value.to_string()),
                        _ => {
                            println!("Invalid attribute command. Attribute parsing failed. Key was {:?}, Value was {:?}", key, value)
                        }
                    }
                    resp.push("200\n".to_string());
                    match sender.send(FrameCommand::Send { data: resp }).await {
                        Ok(_) => {
                            return;
                        }
                        Err(e) => println!(
                            "Error sending to frame task (From: Handle Connection) {:?}",
                            e
                        ),
                    }
                }
                (None, Some(value)) => {}
                (Some(key), None) => {}
                (None, None) => {}
            }
        }
        FtlCommand::Ping => {
            // println!("Handling PING Command");
            resp = Vec::new();
            resp.push("201\n".to_string());
            match sender.send(FrameCommand::Send { data: resp }).await {
                Ok(_) => {
                    return;
                }
                Err(e) => println!(
                    "Error sending to frame task (From: Handle Connection) {:?}",
                    e
                ),
            }
        }
        _ => {
            println!("Command not implemented yet. Tell GRVY to quit his day job");
            return;
        }
    }
}

fn generate_hmac() -> String {
    let dist = Uniform::new(0x00, 0xFF);
    let mut hmac_payload: Vec<u8> = Vec::new();
    let mut rng = thread_rng();
    for _i in 0..128 {
        hmac_payload.push(rng.sample(dist));
    }
    encode(hmac_payload.as_slice())
}