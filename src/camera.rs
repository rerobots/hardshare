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

use std::sync::atomic::{self, AtomicBool};
use std::sync::{mpsc, Arc};
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
    let authheader = format!("Bearer {api_token}");
    let url = format!("{origin}/hardshare/cam/{hscamera_id}/upload");
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
                if let Err(err_from_send) = err_notify.send(format!("failed to open WebSocket: {err}")) {
		    error!("caught error ({err_from_send}) while trying to send notification about error: {err}");
		}
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

// Wrap to make this blocking instead of using a callback
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn initialize_nokhwa() -> Result<(), std::string::String> {
    use nokhwa::nokhwa_initialize;

    let nokhwa_initialized = Arc::new(AtomicBool::new(false));
    let nokhwa_initialized_clone = nokhwa_initialized.clone();

    let init_result = Arc::new(AtomicBool::new(false));
    let init_result_clone = init_result.clone();

    nokhwa_initialize(move |x| {
        init_result_clone.store(x, atomic::Ordering::Relaxed);
        nokhwa_initialized_clone.store(true, atomic::Ordering::Relaxed);
    });

    while !nokhwa_initialized.load(atomic::Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(300));
        // TODO: add timeout
    }

    match init_result.load(atomic::Ordering::Relaxed) {
        true => Ok(()),
        false => Err("nokhwa_initialize() failed".into()),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn verify_capture_ability(
    camera_path: &str,
    dimensions: Option<CameraDimensions>,
) -> Result<(), Box<dyn std::error::Error>> {
    use nokhwa::{
        native_api_backend,
        pixel_format::RgbFormat,
        query,
        utils::{CameraIndex, RequestedFormat, RequestedFormatType, Resolution},
        Camera,
    };

    initialize_nokhwa()?;

    let camera_index: u32 = match camera_path.parse() {
        Ok(c) => c,
        Err(err) => {
            return Err(CheckError::new(format!(
                "error parsing camera index: {err}"
            )));
        }
    };
    debug!("starting camera backend");
    let backend = native_api_backend().ok_or("nokhwa::native_api_backend() failed")?;

    debug!("enumerating camera devices");
    let devices = query(backend)?;

    if camera_index as usize > devices.len() - 1 {
        return Err(CheckError::new(format!(
            "camera index is out of range: {camera_index}"
        )));
    }

    debug!("opening camera {camera_index}");
    let camera_index = CameraIndex::Index(camera_index);
    let format = match dimensions {
        Some(d) => {
            let resolution = Resolution::new(d.width, d.height);
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::HighestResolution(resolution))
        }
        None => RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution),
    };

    let mut dev = match Camera::new(camera_index, format) {
        Ok(d) => d,
        Err(err) => {
            return Err(CheckError::new(format!(
                "failed to open camera device: {err}"
            )));
        }
    };

    dev.open_stream()?;
    dev.frame()?;
    dev.stop_stream()?;

    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn video_capture(
    camera_path: &str,
    dimensions: Option<CameraDimensions>,
    wsclient_addr: Addr<WSClient>,
    cap_command: mpsc::Receiver<CaptureCommand>,
) {
    use std::io::Cursor;

    use nokhwa::{
        pixel_format::RgbFormat,
        utils::{CameraIndex, RequestedFormat, RequestedFormatType, Resolution},
        Camera,
    };

    let camera_index = CameraIndex::Index(match camera_path.parse() {
        Ok(c) => c,
        Err(err) => {
            error!("error parsing camera index: {err}");
            return;
        }
    });

    let (width, height) = match dimensions {
        Some(d) => (d.width, d.height),
        None => (1280, 720),
    };

    let resolution = Resolution::new(width, height);
    let format =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::HighestResolution(resolution));

    let mut dev = match Camera::new(camera_index, format) {
        Ok(d) => d,
        Err(err) => {
            error!("failed to open camera device: {err}");
            return;
        }
    };

    let mut streaming = false;

    loop {
        match cap_command.try_recv() {
            Ok(m) => {
                if m == CaptureCommand::Start {
                    debug!("received start request");
                    if !streaming {
                        if let Err(err) = dev.open_stream() {
                            error!("error starting camera stream: {err}");
                            return;
                        }
                        streaming = true;
                    }
                } else if m == CaptureCommand::Stop {
                    debug!("received stop request");
                    if streaming {
                        if let Err(err) = dev.stop_stream() {
                            error!("error stopping camera stream: {err}");
                            return;
                        }
                        streaming = false;
                    }
                } else {
                    // CaptureCommand::Quit
                    return;
                }
            }
            Err(err) => {
                if err != mpsc::TryRecvError::Empty {
                    error!("caught: {err}");
                    return;
                }
            }
        }

        if streaming {
            let frame = match dev.frame() {
                Ok(f) => f,
                Err(err) => {
                    error!("error capturing frame: {err}");
                    streaming = false;
                    continue;
                }
            };
            match image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_vec(
                width,
                height,
                Vec::from(frame.buffer()),
            ) {
                Some(img) => {
                    let mut jpg: Vec<u8> = Vec::new();
                    if let Err(err) =
                        img.write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg)
                    {
                        error!("ImageBuffer.write_to: {err}");
                        continue;
                    }

                    let b64data = base64_engine::STANDARD.encode(jpg);
                    if let Err(err) = wsclient_addr
                        .try_send(WSSend("data:image/jpeg;base64,".to_string() + &b64data))
                    {
                        error!("try_send failed; caught: {err:?}");
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
                    Err(err) => {
                        error!("caught while attempting to close camera WebSocket: {err:?}")
                    }
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
                    if let Err(err) = self.capture.send(CaptureCommand::Start) {
                        error!("caught while trying to send CaptureCommand::Start: {err}");
                    }
                } else if txt == "STOP" {
                    if let Err(err) = self.capture.send(CaptureCommand::Stop) {
                        error!("caught while trying to send CaptureCommand::Stop: {err}");
                    }
                } else {
                    warn!("unrecognized WebSocket message: {txt:?}");
                }
            }
            Ok(Frame::Ping(_)) => {
                debug!("received PING; sending PONG");
                match self.ws_sink.write(Message::Pong(Bytes::from_static(b""))) {
                    Ok(()) => (),
                    Err(err) => error!("caught while responding to WebSocket ping: {err:?}"),
                }
            }
            Ok(_) => {
                warn!("unrecognized WebSocket message: {msg:?}");
            }
            Err(err) => {
                error!("caught {err:?}");
                ctx.stop();
            }
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        debug!("closing WebSocket");
        if let Err(err) = self.capture.send(CaptureCommand::Quit) {
            error!("caught while closing WebSocket: {err}");
        }
        self.ws_sink.close();
        ctx.stop()
    }
}

impl Handler<WSSend> for WSClient {
    type Result = ();

    fn handle(&mut self, msg: WSSend, _ctx: &mut Context<Self>) {
        match self.ws_sink.write(Message::Text(msg.0.into())) {
            Ok(()) => (),
            Err(err) => {
                error!("caught while attempting to send message via camera WebSocket: {err:?}")
            }
        }
        self.recent_txrx_instant = std::time::Instant::now();
    }
}

impl actix::io::WriteHandler<WsProtocolError> for WSClient {}
