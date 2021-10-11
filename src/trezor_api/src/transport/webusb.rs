use crate::transport::{
    AvailableDeviceTransport, Error, Link, ProtoMessage, Protocol, ProtocolV1, Transport,
};
use crate::{AvailableDevice, TrezorModel};
use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use js_sys::{Array, ArrayBuffer, Promise, Uint8Array};
use std::fmt;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

/// The chunk size for the serial protocol.
const CHUNK_SIZE: usize = 64;

type ActionResultSender<T> = oneshot::Sender<TransportResult<T>>;
type ActionSender = mpsc::UnboundedSender<WebUsbAction>;

type TransportResult<T> = std::result::Result<T, Error>;

async fn send_event_recv_response<Event, Result>(
    event_tx: &mpsc::UnboundedSender<Event>,
    event: Event,
    result_rx: oneshot::Receiver<TransportResult<Result>>,
) -> TransportResult<Result> {
    if let Err(e) = event_tx.unbounded_send(event) {
        let error = format!("Error sending event: {}", e);
        return Err(Error::Internal(error));
    }
    match result_rx.await {
        Ok(result) => result,
        Err(e) => {
            let error = format!("Error receiving result: {}", e);
            Err(Error::Internal(error))
        }
    }
}

#[derive(Debug)]
enum WebUsbAction {
    RequestDevice {
        result_tx: ActionResultSender<()>,
    },
    FindDevices {
        result_tx: ActionResultSender<Vec<AvailableWebUsbDevice>>,
    },
    WriteChunk {
        path: String,
        chunk: Vec<u8>,
        result_tx: ActionResultSender<()>,
    },
    ReadChunk {
        path: String,
        result_tx: ActionResultSender<Vec<u8>>,
    },
}

async fn init_webusb_plugin() -> Result<ActionSender, Error> {
    let (tx, mut rx) = mpsc::unbounded();
    let plugin = TrezorWebUsbPlugin::new()
        .map_err(|e| Error::WebUsb(format!("Error initializing WebUSB: {:?}", e)))?;

    let fut = async move {
        while let Some(action) = rx.next().await {
            match action {
                WebUsbAction::RequestDevice { result_tx } => {
                    result_tx.send(on_request_device(&plugin).await).ok();
                }
                WebUsbAction::FindDevices { result_tx } => {
                    result_tx.send(on_find_devices(&plugin).await).ok();
                }
                WebUsbAction::WriteChunk {
                    path,
                    chunk,
                    result_tx,
                } => {
                    result_tx
                        .send(on_write_chunk(&plugin, path, chunk).await)
                        .ok();
                }
                WebUsbAction::ReadChunk { path, result_tx } => {
                    result_tx.send(on_read_chunk(&plugin, path).await).ok();
                }
            }
        }
    };
    spawn_local(fut);
    Ok(tx)
}

async fn on_request_device(plugin: &TrezorWebUsbPlugin) -> TransportResult<()> {
    plugin
        .request_device()
        .await
        .map_err(|e| Error::WebUsb(format!("Error getting devices: {:?}", e)))
}

async fn on_find_devices(
    plugin: &TrezorWebUsbPlugin,
) -> TransportResult<Vec<AvailableWebUsbDevice>> {
    let js_value = plugin
        .enumerate()
        .await
        .map_err(|e| Error::WebUsb(format!("Error getting devices: {:?}", e)))?;
    js_value.into_serde().map_err(|e| {
        Error::WebUsb(format!(
            "Error deserializing the list of available devices: {:?}",
            e
        ))
    })
}

async fn on_write_chunk(
    plugin: &TrezorWebUsbPlugin,
    path: String,
    chunk: Vec<u8>,
) -> TransportResult<()> {
    let data = Uint8Array::from(chunk.as_slice());
    plugin
        .send(&path, data)
        .await
        .map_err(|e| Error::WebUsb(format!("Error writing a chunk: {:?}", e)))
}

async fn on_read_chunk(plugin: &TrezorWebUsbPlugin, path: String) -> TransportResult<Vec<u8>> {
    let js_value = plugin
        .receive(&path)
        .await
        .map_err(|e| Error::WebUsb(format!("Error reading a chunk: {:?}", e)))?;
    let buf = js_value
        .dyn_into::<ArrayBuffer>()
        .map_err(|data| Error::WebUsb(format!("Expected 'ArrayBuffer', found: {:?}", data)))?;
    let buf = Uint8Array::new(&buf);
    let chunk = buf.to_vec();
    crate::console_log!("Received chunk: {:?}", chunk);
    // if chunk.len() != CHUNK_SIZE {
    //     return Err(Error::WebUsb(format!(
    //         "Received an invalid chunk: {:?}",
    //         chunk
    //     )));
    // }
    Ok(chunk)
}

#[rustfmt::skip]
#[wasm_bindgen(raw_module = "../../../js/trezor-webusb-plugin.js")]
extern "C" {
    type TrezorWebUsbPlugin;

    #[wasm_bindgen(catch, constructor)]
    fn new() -> Result<TrezorWebUsbPlugin, JsValue>;

    /// `wasm_bindgen` allows to return `JsValue` from extern async functions only.
    /// Ok - Vec<JsValue>
    #[wasm_bindgen(catch, method)]
    async fn enumerate(this: &TrezorWebUsbPlugin) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, method)]
    async fn send(
        this: &TrezorWebUsbPlugin,
        path: &str,
        data: Uint8Array,
    ) -> Result<(), JsValue>;

    /// `wasm_bindgen` allows to return `JsValue` from extern async functions only.
    /// Ok - ArrayBuffer
    #[wasm_bindgen(catch, method)]
    async fn receive(this: &TrezorWebUsbPlugin, path: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, method)]
    async fn connect(
        this: &TrezorWebUsbPlugin,
        path: &str,
        first: bool,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method)]
    async fn disconnect(
        this: &TrezorWebUsbPlugin,
        path: &str,
        last: bool,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method, js_name = requestDevice)]
    async fn request_device(this: &TrezorWebUsbPlugin) -> Result<(), JsValue>;
}

pub struct WebUsbLink {
    event_tx: ActionSender,
    device: AvailableWebUsbDevice,
}

#[async_trait]
impl Link for WebUsbLink {
    async fn write_chunk(&mut self, chunk: Vec<u8>) -> Result<(), Error> {
        debug_assert_eq!(CHUNK_SIZE, chunk.len());
        let (result_tx, result_rx) = oneshot::channel();
        send_event_recv_response(
            &self.event_tx,
            WebUsbAction::WriteChunk {
                path: self.device.path.clone(),
                chunk,
                result_tx,
            },
            result_rx,
        )
        .await
    }

    async fn read_chunk(&mut self) -> Result<Vec<u8>, Error> {
        let (result_tx, result_rx) = oneshot::channel();
        let path = self.device.path.clone();
        send_event_recv_response(
            &self.event_tx,
            WebUsbAction::ReadChunk { path, result_tx },
            result_rx,
        )
        .await
    }
}

/// An implementation of the Transport interface for WebUSB devices.
pub struct WebUsbTransport {
    protocol: ProtocolV1<WebUsbLink>,
}

#[async_trait]
impl Transport for WebUsbTransport {
    async fn session_begin(&mut self) -> Result<(), Error> {
        self.protocol.session_begin().await
    }

    async fn session_end(&mut self) -> Result<(), Error> {
        self.protocol.session_end().await
    }

    async fn write_message(&mut self, message: ProtoMessage) -> Result<(), Error> {
        self.protocol.write(message).await
    }

    async fn read_message(&mut self) -> Result<ProtoMessage, Error> {
        self.protocol.read().await
    }
}

impl WebUsbTransport {
    pub async fn find_devices() -> Result<Vec<AvailableDevice>, Error> {
        let mut event_sender = init_webusb_plugin().await?;
        WebUsbTransport::request_device(&event_sender).await?;

        let (result_tx, mut result_rx) = oneshot::channel();
        send_event_recv_response(
            &event_sender,
            WebUsbAction::FindDevices { result_tx },
            result_rx,
        )
        .await
        .map(|devices| {
            devices
                .into_iter()
                .map(|device| {
                    let debug = device.debug;
                    let webusb_transport = AvailableWebUsbTransport {
                        event_tx: event_sender.clone(),
                        device,
                    };
                    let model = TrezorModel::T;
                    let transport = AvailableDeviceTransport::WebUsb(webusb_transport);
                    AvailableDevice {
                        model,
                        debug,
                        transport,
                    }
                })
                .collect()
        })
    }

    /// Similar to `UsbTransport::connect`.
    pub fn connect(device: &AvailableDevice) -> Result<Box<dyn Transport>, Error> {
        let transport = match device.transport {
            AvailableDeviceTransport::WebUsb(ref t) => t,
            _ => panic!("passed wrong AvailableDevice in WebUsbTransport::connect"),
        };
        Ok(Box::new(WebUsbTransport {
            protocol: ProtocolV1 {
                link: WebUsbLink {
                    device: transport.device.clone(),
                    event_tx: transport.event_tx.clone(),
                },
            },
        }))
    }

    async fn request_device(event_tx: &ActionSender) -> Result<(), Error> {
        let (result_tx, mut result_rx) = oneshot::channel();
        send_event_recv_response(
            event_tx,
            WebUsbAction::RequestDevice { result_tx },
            result_rx,
        )
        .await
    }
}

#[derive(Clone, Debug, Deserialize)]
struct AvailableWebUsbDevice {
    path: String,
    debug: bool,
}

#[derive(Debug)]
pub struct AvailableWebUsbTransport {
    device: AvailableWebUsbDevice,
    event_tx: ActionSender,
}

impl fmt::Display for AvailableWebUsbTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.device)
    }
}

mod tests {
    use super::*;
    use wasm_bindgen_test::*;
    use web_sys::console;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_trezor_webusb_plugin() {
        let plugin = TrezorWebUsbPlugin::new().unwrap();
        // pluging.enumerate()
    }
}
