// Copyright (C) 2023 rerobots, Inc.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use actix::io::SinkWrite;
use actix::prelude::*;
use actix_codec::Framed;
use awc::{
    error::WsProtocolError,
    ws::{Codec, Frame, Message},
    BoxedSocket,
};

use bytes::Bytes;
use futures::stream::{SplitSink, StreamExt};

use openssl::ssl::{SslConnector, SslMethod};

#[cfg(target_os="linux")]
use v4l::io::traits::CaptureStream;

#[cfg(target_os="linux")]
use v4l::prelude::*;

#[cfg(target_os="linux")]
use v4l::video::Capture;

use crate::api;


#[cfg(target_os="linux")]
pub fn stream_websocket(
    origin: &str,
    api_token: &str,
    hscamera_id: &str,
    camera_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let camera_path = String::from(camera_path);
    let authheader = format!("Bearer {}", api_token);
    let url = format!("{}/hardshare/cam/{}/upload", origin, hscamera_id);
    let sys = System::new("wsclient");
    let (err_notify, err_rx) = mpsc::channel();
    Arbiter::spawn(async move {
        let ssl_builder = match SslConnector::builder(SslMethod::tls()) {
            Ok(s) => s,
            Err(err) => {
                err_notify
                    .send(format!("failed to open WebSocket: {}", err))
                    .unwrap();
                System::current().stop_with_code(1);
                return;
            }
        };
        let connector = ssl_builder.build();
        let client = awc::Client::builder()
            .connector(awc::Connector::new().ssl(connector).finish())
            .header("Authorization", authheader)
            .finish();

        let (_, framed) = match client.ws(url).connect().await {
            Ok(c) => c,
            Err(err) => {
                err_notify
                    .send(format!("failed to open WebSocket: {}", err))
                    .unwrap();
                System::current().stop_with_code(1);
                return;
            }
        };

        let (sink, stream) = framed.split();

        let (capture_tx, capture_rx) = mpsc::channel();
        let addr = WSClient::create(|ctx| {
            WSClient::add_stream(stream, ctx);
            WSClient {
                ws_sink: SinkWrite::new(sink, ctx),
                recent_txrx_instant: std::time::Instant::now(), // First instant at first connect
                capture: capture_tx,
            }
        });
        std::thread::spawn(move || video_capture(&camera_path, addr, capture_rx));
    });
    match sys.run() {
        Ok(()) => Ok(()),
        Err(_) => api::error(err_rx.recv()?),
    }
}


#[derive(PartialEq)]
enum CaptureCommand {
    Start, // Read images from camera
    Stop,  // Do not read images from camera
    Quit,  // Return from (close) the thread
}


#[cfg(target_os="linux")]
fn video_capture(
    camera_path: &str,
    wsclient_addr: Addr<WSClient>,
    cap_command: mpsc::Receiver<CaptureCommand>,
) {
    let buffer_count = 4;
    let dev = match v4l::Device::with_path(camera_path) {
        Ok(d) => d,
        Err(err) => {
            error!("when opening camera device, caught {}", err);
            return;
        }
    };
    let mut format = dev.format().unwrap();
    format.fourcc = v4l::FourCC::new(b"MJPG");
    format = match dev.set_format(&format) {
        Ok(f) => f,
        Err(err) => {
            error!("failed to set camera format MJPG: {}", err);
            return;
        }
    };
    let mut stream = None;

    loop {
        match cap_command.try_recv() {
            Ok(m) => {
                if m == CaptureCommand::Start {
                    if stream.is_none() {
                        let s = match MmapStream::with_buffers(
                            &dev,
                            v4l::buffer::Type::VideoCapture,
                            buffer_count,
                        ) {
                            Ok(s) => s,
                            Err(err) => {
                                error!("failed to open stream: {}", err);
                                return;
                            }
                        };
                        stream = Some(s);
                    }
                } else if m == CaptureCommand::Stop {
                    stream = None;
                } else {
                    // CaptureCommand::Quit
                    return;
                }
            }
            Err(err) => {
                if err != mpsc::TryRecvError::Empty {
                    error!("caught: {}", err);
                    return;
                }
            }
        }

        if let Some(s) = &mut stream {
            let (buf, _) = match s.next() {
                Ok(i) => i,
                Err(err) => {
                    error!("error reading camera stream: {}", err);
                    return;
                }
            };
            let data = buf.to_vec();
            let b64data = base64::encode(data);
            if let Err(err) = wsclient_addr.try_send(WSSend("data:image/jpeg;base64,".to_string() + &b64data)) {
                error!("try_send failed; caught: {:?}", err);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        } else {
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
}


struct WSClient {
    ws_sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    recent_txrx_instant: std::time::Instant,
    capture: mpsc::Sender<CaptureCommand>,
}

#[derive(Message)]
#[rtype(result = "()")]
struct WSSend(String);

impl Actor for WSClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.check_receive_timeout(ctx);
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        debug!("WSClient actor stopped");
    }
}

impl WSClient {
    fn check_receive_timeout(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(60, 0), |act, ctx| {
            if act.recent_txrx_instant.elapsed() > Duration::new(45, 0) {
                debug!("timeout waiting for server");
                act.ws_sink.write(Message::Close(None));
                ctx.stop();
            } else {
                act.check_receive_timeout(ctx);
            }
        });
    }
}

impl StreamHandler<Result<Frame, WsProtocolError>> for WSClient {
    fn handle(&mut self, msg: Result<Frame, WsProtocolError>, ctx: &mut Context<Self>) {
        self.recent_txrx_instant = std::time::Instant::now();

        match msg {
            Ok(Frame::Text(txt)) => {
                if txt == "START" {
                    self.capture.send(CaptureCommand::Start).unwrap();
                } else if txt == "STOP" {
                    self.capture.send(CaptureCommand::Stop).unwrap();
                } else {
                    warn!("unrecognized WebSocket message: {:?}", txt);
                }
            }
            Ok(Frame::Ping(_)) => {
                debug!("received PING; sending PONG");
                self.ws_sink.write(Message::Pong(Bytes::from_static(b"")));
            }
            Ok(_) => {
                warn!("unrecognized WebSocket message: {:?}", msg);
            }
            Err(err) => {
                error!("caught {:?}", err);
                ctx.stop();
            }
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        self.capture.send(CaptureCommand::Quit).unwrap();
        self.ws_sink.close();
        ctx.stop()
    }
}

impl Handler<WSSend> for WSClient {
    type Result = ();

    fn handle(&mut self, msg: WSSend, ctx: &mut Context<Self>) {
        self.ws_sink.write(Message::Text(msg.0));
        self.recent_txrx_instant = std::time::Instant::now();
    }
}

impl actix::io::WriteHandler<WsProtocolError> for WSClient {}
