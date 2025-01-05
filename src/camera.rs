// Copyright (C) 2023 rerobots, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::mpsc;
use std::time::Duration;

use actix::io::SinkWrite;
use actix::prelude::*;
use actix_codec::Framed;
use actix_web::web::Bytes;
use awc::{
    error::WsProtocolError,
    ws::{Codec, Frame, Message},
    BoxedSocket,
};

use base64::engine::{general_purpose as base64_engine, Engine as _};
use futures::stream::{SplitSink, StreamExt};

use crate::api::{self, CameraDimensions};
use crate::check::Error as CheckError;

pub fn get_default_dev() -> String {
    #[cfg(target_os = "linux")]
    return "/dev/video0".into();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    return "0".into();
}

pub fn check_camera(camera_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    verify_capture_ability(camera_path, None)
}

pub fn stream_websocket(
    origin: &str,
    api_token: &str,
    hscamera_id: &str,
    camera_path: &str,
    dimensions: &Option<CameraDimensions>,
) -> Result<(), Box<dyn std::error::Error>> {
    let camera_path = String::from(camera_path);
    let dimensions = dimensions.as_ref().cloned();
    let authheader = format!("Bearer {}", api_token);
    let url = format!("{}/hardshare/cam/{}/upload", origin, hscamera_id);
    let sys = System::new();
    let (err_notify, err_rx) = mpsc::channel();
    sys.runtime().spawn(async move {
        let client = awc::Client::builder()
            .add_default_header(("Authorization", authheader))
            .finish();

        debug!("opening camera websocket...");
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
        debug!("camera websocket opened");

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
        std::thread::spawn(move || video_capture(&camera_path, dimensions, addr, capture_rx));
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

#[cfg(target_os = "macos")]
fn verify_capture_ability(
    camera_path: &str,
    dimensions: Option<CameraDimensions>,
) -> Result<(), Box<dyn std::error::Error>> {
    use openpnp_capture::{Device, Format, Stream};

    let camera_index: usize = match camera_path.parse() {
        Ok(c) => c,
        Err(err) => {
            return Err(CheckError::new(format!(
                "error parsing camera index: {}",
                err
            )));
        }
    };
    debug!("enumerating camera devices");
    let devices = Device::enumerate();

    debug!("opening camera {}", camera_index);
    if camera_index > devices.len() - 1 {
        return Err(CheckError::new(format!(
            "camera index is out of range: {}",
            camera_index
        )));
    }
    let dev = match Device::new(devices[camera_index]) {
        Some(d) => d,
        None => {
            return Err(CheckError::new("failed to open camera device"));
        }
    };

    let (mut width, mut height) = match dimensions {
        Some(d) => (d.width, d.height),
        None => (1280, 720),
    };
    let format = Format::default().width(width).height(height);

    let stream = match Stream::new(&dev, &format) {
        Some(s) => s,
        None => {
            return Err(CheckError::new("failed to create camera stream"));
        }
    };
    if stream.format().width != width || stream.format().height != height {
        (width, height) = (stream.format().width, stream.format().height);
        warn!(
            "requested format not feasible; falling back to ({}, {})",
            width, height
        );
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn video_capture(
    camera_path: &str,
    dimensions: Option<CameraDimensions>,
    wsclient_addr: Addr<WSClient>,
    cap_command: mpsc::Receiver<CaptureCommand>,
) {
    use std::io::Cursor;

    use openpnp_capture::{Device, Format, Stream};

    let camera_index: usize = match camera_path.parse() {
        Ok(c) => c,
        Err(err) => {
            error!("error parsing camera index: {}", err);
            return;
        }
    };
    debug!("enumerating camera devices");
    let devices = Device::enumerate();

    debug!("opening camera {}", camera_index);
    let dev = match Device::new(devices[camera_index]) {
        Some(d) => d,
        None => {
            error!("failed to open camera device");
            return;
        }
    };

    let (mut width, mut height) = match dimensions {
        Some(d) => (d.width, d.height),
        None => (1280, 720),
    };
    let mut buf_capacity: usize = (width as usize) * (height as usize) * 3;
    let mut format = Format::default().width(width).height(height);
    let mut stream = None;

    loop {
        match cap_command.try_recv() {
            Ok(m) => {
                if m == CaptureCommand::Start {
                    debug!("received start request");
                    if stream.is_none() {
                        let s = match Stream::new(&dev, &format) {
                            Some(s) => s,
                            None => {
                                error!("failed to create camera stream");
                                return;
                            }
                        };
                        if s.format().width != width || s.format().height != height {
                            (width, height) = (s.format().width, s.format().height);
                            buf_capacity = (width as usize) * (height as usize) * 3;
                            format = Format::default().width(width).height(height);
                            warn!(
                                "requested format not feasible; falling back to ({}, {})",
                                width, height
                            );
                        }

                        stream = Some(s);
                    }
                } else if m == CaptureCommand::Stop {
                    debug!("received stop request");
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
            s.advance();
            let mut data = vec![0; buf_capacity];
            if let Err(err) = s.read(&mut data) {
                error!("error reading camera stream: {}", err);
                return;
            }

            match image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_vec(width, height, data) {
                Some(img) => {
                    let mut jpg: Vec<u8> = Vec::new();
                    img.write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg)
                        .unwrap();

                    let b64data = base64_engine::STANDARD.encode(jpg);
                    if let Err(err) = wsclient_addr
                        .try_send(WSSend("data:image/jpeg;base64,".to_string() + &b64data))
                    {
                        error!("try_send failed; caught: {:?}", err);
                    }
                }
                None => warn!("failed to decode camera image"),
            }
        } else {
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}

#[cfg(target_os = "windows")]
fn verify_capture_ability(
    camera_path: &str,
    dimensions: Option<CameraDimensions>,
) -> Result<(), Box<dyn std::error::Error>> {
    return Err(CheckError::new("cameras not supported on Windows"));
}

#[cfg(target_os = "windows")]
fn video_capture(
    camera_path: &str,
    dimensions: Option<CameraDimensions>,
    wsclient_addr: Addr<WSClient>,
    cap_command: mpsc::Receiver<CaptureCommand>,
) {
}

#[cfg(target_os = "linux")]
fn verify_capture_ability(
    camera_path: &str,
    dimensions: Option<CameraDimensions>,
) -> Result<(), Box<dyn std::error::Error>> {
    use v4l::prelude::*;
    use v4l::video::Capture;

    let buffer_count = 4;
    debug!("opening camera {}", camera_path);
    let dev = match v4l::Device::with_path(camera_path) {
        Ok(d) => d,
        Err(err) => {
            return Err(CheckError::new(format!(
                "when opening camera device, caught {}",
                err
            )));
        }
    };
    let mut format = dev.format().unwrap();
    format.fourcc = v4l::FourCC::new(b"MJPG");
    if let Some(d) = &dimensions {
        format.width = d.width;
        format.height = d.height;
    }
    match dev.set_format(&format) {
        Ok(f) => {
            if let Some(d) = dimensions {
                if f.width != d.width || f.height != d.height {
                    warn!(
                        "requested size not feasible; falling back to ({}, {})",
                        f.width, f.height
                    );
                }
            }
            debug!("set format: {}", f);
            f
        }
        Err(err) => {
            return Err(CheckError::new(format!(
                "failed to set camera format MJPG: {}",
                err
            )));
        }
    };

    match MmapStream::with_buffers(&dev, v4l::buffer::Type::VideoCapture, buffer_count) {
        Ok(s) => {
            debug!("MmapStream, video capture");
            s
        }
        Err(err) => {
            return Err(CheckError::new(format!("failed to open stream: {}", err)));
        }
    };

    Ok(())
}

#[cfg(target_os = "linux")]
fn video_capture(
    camera_path: &str,
    dimensions: Option<CameraDimensions>,
    wsclient_addr: Addr<WSClient>,
    cap_command: mpsc::Receiver<CaptureCommand>,
) {
    use v4l::io::traits::CaptureStream;
    use v4l::prelude::*;
    use v4l::video::Capture;

    let buffer_count = 4;
    debug!("opening camera {}", camera_path);
    let dev = match v4l::Device::with_path(camera_path) {
        Ok(d) => d,
        Err(err) => {
            error!("when opening camera device, caught {}", err);
            return;
        }
    };
    let mut format = dev.format().unwrap();
    format.fourcc = v4l::FourCC::new(b"MJPG");
    if let Some(d) = &dimensions {
        format.width = d.width;
        format.height = d.height;
    }
    match dev.set_format(&format) {
        Ok(f) => {
            if let Some(d) = dimensions {
                if f.width != d.width || f.height != d.height {
                    warn!(
                        "requested size not feasible; falling back to ({}, {})",
                        f.width, f.height
                    );
                }
            }
            debug!("set format: {}", f);
            f
        }
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
                    debug!("received start request");
                    if stream.is_none() {
                        let s = match MmapStream::with_buffers(
                            &dev,
                            v4l::buffer::Type::VideoCapture,
                            buffer_count,
                        ) {
                            Ok(s) => {
                                debug!("MmapStream, video capture");
                                s
                            }
                            Err(err) => {
                                error!("failed to open stream: {}", err);
                                return;
                            }
                        };
                        stream = Some(s);
                    }
                } else if m == CaptureCommand::Stop {
                    debug!("received stop request");
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
            let (buf, metadata) = match s.next() {
                Ok(i) => i,
                Err(err) => {
                    error!("error reading camera stream: {}", err);
                    return;
                }
            };
            debug!(
                "metadata: bytesused {}, sequence {}, flags {}, length {}",
                metadata.bytesused,
                metadata.sequence,
                metadata.flags,
                buf.len()
            );
            let data = buf.to_vec();
            let b64data = base64_engine::STANDARD.encode(data);
            debug!("sending frame");
            if let Err(err) =
                wsclient_addr.try_send(WSSend("data:image/jpeg;base64,".to_string() + &b64data))
            {
                error!("try_send failed; caught: {:?}", err);
            }
            // TODO: This is too slow! The WebSocket connection is lost on
            // some machines when this sleep duration is too small. Why?
            std::thread::sleep(Duration::from_millis(200));
        } else {
            std::thread::sleep(Duration::from_secs(2));
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

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        debug!("WSClient actor stopped");
    }
}

impl WSClient {
    fn check_receive_timeout(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(60, 0), |act, ctx| {
            if act.recent_txrx_instant.elapsed() > Duration::new(45, 0) {
                debug!("timeout waiting for server");
                match act.ws_sink.write(Message::Close(None)) {
                    Ok(()) => (),
                    Err(err) => error!(
                        "caught while attempting to close camera WebSocket: {:?}",
                        err
                    ),
                }
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
                match self.ws_sink.write(Message::Pong(Bytes::from_static(b""))) {
                    Ok(()) => (),
                    Err(err) => error!("caught while responding to WebSocket ping: {:?}", err),
                }
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
        debug!("closing WebSocket");
        self.capture.send(CaptureCommand::Quit).unwrap();
        self.ws_sink.close();
        ctx.stop()
    }
}

impl Handler<WSSend> for WSClient {
    type Result = ();

    fn handle(&mut self, msg: WSSend, _ctx: &mut Context<Self>) {
        match self.ws_sink.write(Message::Text(msg.0.into())) {
            Ok(()) => (),
            Err(err) => error!(
                "caught while attempting to send message via camera WebSocket: {:?}",
                err
            ),
        }
        self.recent_txrx_instant = std::time::Instant::now();
    }
}

impl actix::io::WriteHandler<WsProtocolError> for WSClient {}
