use std::fmt;

use super::{protos, Error, Result, TrezorModel};
use crate::messages::TrezorMessage;
use crate::transport::{ProtoMessage, Transport};
use protos::Address as BitcoinAddress;
use protos::KeyDerivationPath;
use protos::MessageType::*;

// Some types with raw protos that we use in the public interface so they have to be exported.
pub use protos::ButtonRequest_ButtonRequestType as ButtonRequestType;
pub use protos::Features;
pub use protos::PinMatrixRequest_PinMatrixRequestType as PinMatrixRequestType;
pub use protos::{TezosAddress, TezosPublicKey, TezosSignTx, TezosSignedTx};

/// The different options for the number of words in a seed phrase.
pub enum WordCount {
    W12 = 12,
    W18 = 18,
    W24 = 24,
}

/// The different types of user interactions the Trezor device can request.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum InteractionType {
    Button,
    PinMatrix,
    Passphrase,
    PassphraseState,
}

//TODO(stevenroose) should this be FnOnce and put in an FnBox?
/// Function to be passed to the `Trezor.call` method to process the
/// Trezor response message into a general-purpose type.
pub type ResultHandler<'a, T, R> = dyn Fn(&'a mut Trezor, R) -> Result<T>;

/// A button request message sent by the device.
pub struct ButtonRequest<'a, T, R: TrezorMessage> {
    message: protos::ButtonRequest,
    client: &'a mut Trezor,
    result_handler: Box<ResultHandler<'a, T, R>>,
}

impl<'a, T, R: TrezorMessage> fmt::Debug for ButtonRequest<'a, T, R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.message, f)
    }
}

impl<'a, T, R: TrezorMessage> ButtonRequest<'a, T, R> {
    /// The type of button request.
    pub fn request_type(&self) -> ButtonRequestType {
        self.message.get_code()
    }

    /// Ack the request and get the next message from the device.
    pub async fn ack(self) -> Result<TrezorResponse<'a, T, R>> {
        let req = protos::ButtonAck::new();
        self.client.call(req, self.result_handler).await
    }
}

/// A PIN matrix request message sent by the device.
pub struct PinMatrixRequest<'a, T, R: TrezorMessage> {
    message: protos::PinMatrixRequest,
    client: &'a mut Trezor,
    result_handler: Box<ResultHandler<'a, T, R>>,
}

impl<'a, T, R: TrezorMessage> fmt::Debug for PinMatrixRequest<'a, T, R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.message, f)
    }
}

impl<'a, T, R: TrezorMessage> PinMatrixRequest<'a, T, R> {
    /// The type of PIN matrix request.
    pub fn request_type(&self) -> PinMatrixRequestType {
        self.message.get_field_type()
    }

    /// Ack the request with a PIN and get the next message from the device.
    pub async fn ack_pin(self, pin: String) -> Result<TrezorResponse<'a, T, R>> {
        let mut req = protos::PinMatrixAck::new();
        req.set_pin(pin);
        self.client.call(req, self.result_handler).await
    }
}

/// A response from a Trezor device.
///
/// On every message exchange, instead of the expected/desired response,
/// the Trezor can ask for some user interaction, or can send a failure.
#[derive(Debug)]
pub enum TrezorResponse<'a, T, R: TrezorMessage> {
    Ok(T),
    Failure(protos::Failure),
    ButtonRequest(ButtonRequest<'a, T, R>),
    PinMatrixRequest(PinMatrixRequest<'a, T, R>),
}

impl<'a, T, R: TrezorMessage> fmt::Display for TrezorResponse<'a, T, R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TrezorResponse::Ok(ref _m) => write!(f, "Ok"), //TODO(stevenroose) should we make T: Debug?
            TrezorResponse::Failure(ref m) => write!(f, "Failure: {:?}", m),
            TrezorResponse::ButtonRequest(ref r) => write!(f, "ButtonRequest: {:?}", r),
            TrezorResponse::PinMatrixRequest(ref r) => write!(f, "PinMatrixRequest: {:?}", r),
        }
    }
}

impl<'a, T, R: TrezorMessage> TrezorResponse<'a, T, R> {
    /// Get the actual `Ok` response value or an error if not `Ok`.
    pub fn ok(self) -> Result<T> {
        match self {
            TrezorResponse::Ok(m) => Ok(m),
            TrezorResponse::Failure(m) => Err(Error::FailureResponse(m)),
            TrezorResponse::ButtonRequest(_) => {
                Err(Error::UnexpectedInteractionRequest(InteractionType::Button))
            }
            TrezorResponse::PinMatrixRequest(_) => Err(Error::UnexpectedInteractionRequest(
                InteractionType::PinMatrix,
            )),
        }
    }

    /// Get the button request object or an error if not `ButtonRequest`.
    pub fn button_request(self) -> Result<ButtonRequest<'a, T, R>> {
        match self {
            TrezorResponse::ButtonRequest(r) => Ok(r),
            TrezorResponse::Ok(_) => Err(Error::UnexpectedMessageType(R::message_type())),
            TrezorResponse::Failure(m) => Err(Error::FailureResponse(m)),
            TrezorResponse::PinMatrixRequest(_) => Err(Error::UnexpectedInteractionRequest(
                InteractionType::PinMatrix,
            )),
        }
    }

    /// Get the PIN matrix request object or an error if not `PinMatrixRequest`.
    pub fn pin_matrix_request(self) -> Result<PinMatrixRequest<'a, T, R>> {
        match self {
            TrezorResponse::PinMatrixRequest(r) => Ok(r),
            TrezorResponse::Ok(_) => Err(Error::UnexpectedMessageType(R::message_type())),
            TrezorResponse::Failure(m) => Err(Error::FailureResponse(m)),
            TrezorResponse::ButtonRequest(_) => {
                Err(Error::UnexpectedInteractionRequest(InteractionType::Button))
            }
        }
    }

    /// Ack all requests and return final `Result`.
    ///
    /// Will error if it receives requests, which require input
    /// like: `PinMatrixRequest`.
    pub async fn ack_all(self) -> Result<T> {
        let mut resp = self;
        loop {
            resp = match resp {
                Self::Ok(val) => {
                    return Ok(val);
                }
                Self::Failure(err) => {
                    return Err(Error::FailureResponse(err));
                }
                Self::ButtonRequest(req) => req.ack().await?,
                Self::PinMatrixRequest(_) => {
                    return Err(Error::UnexpectedInteractionRequest(
                        InteractionType::PinMatrix,
                    ));
                }
            };
        }
    }
}

/// When resetting the device, it will ask for entropy to aid key generation.
pub struct EntropyRequest<'a> {
    client: &'a mut Trezor,
}

impl<'a> EntropyRequest<'a> {
    /// Provide exactly 32 bytes or entropy.
    pub async fn ack_entropy(
        self,
        entropy: Vec<u8>,
    ) -> Result<TrezorResponse<'a, (), protos::Success>> {
        if entropy.len() != 32 {
            return Err(Error::InvalidEntropy);
        }

        let mut req = protos::EntropyAck::new();
        req.set_entropy(entropy);
        self.client.call(req, Box::new(|_, _| Ok(()))).await
    }
}

/// A Trezor client.
pub struct Trezor {
    model: TrezorModel,
    // Cached features for later inspection.
    features: Option<protos::Features>,
    transport: Box<dyn Transport>,
}

/// Create a new Trezor instance with the given transport.
pub fn trezor_with_transport(model: TrezorModel, transport: Box<dyn Transport>) -> Trezor {
    Trezor {
        model,
        transport,
        features: None,
    }
}

impl Trezor {
    /// Get the model of the Trezor device.
    pub fn model(&self) -> TrezorModel {
        self.model
    }

    /// Get the features of the Trezor device.
    pub fn features(&self) -> Option<&protos::Features> {
        self.features.as_ref()
    }

    /// Sends a message and returns the raw ProtoMessage struct that was
    /// responded by the device.
    ///
    /// This method is only exported for users that want to expand the
    /// features of this library f.e. for supporting additional coins etc.
    pub async fn call_raw<S: TrezorMessage>(&mut self, message: S) -> Result<ProtoMessage> {
        let proto_msg = ProtoMessage(S::message_type(), message.write_to_bytes()?);
        self.transport
            .write_message(proto_msg)
            .await
            .map_err(|e| Error::TransportSendMessage(e))?;
        self.transport
            .read_message()
            .await
            .map_err(|e| Error::TransportReceiveMessage(e))
    }

    /// Sends a message and returns a TrezorResponse with either the
    /// expected response message, a failure or an interaction request.
    ///
    /// This method is only exported for users that want to expand the
    /// features of this library f.e. for supporting additional coins etc.
    pub async fn call<'a, T, S: TrezorMessage, R: TrezorMessage>(
        &'a mut self,
        message: S,
        result_handler: Box<ResultHandler<'a, T, R>>,
    ) -> Result<TrezorResponse<'a, T, R>> {
        // trace!("Sending {:?} msg: {:?}", S::message_type(), message);
        let resp = self.call_raw(message).await?;
        if resp.message_type() == R::message_type() {
            let resp_msg = resp.into_message()?;
            // trace!("Received {:?} msg: {:?}", R::message_type(), resp_msg);
            Ok(TrezorResponse::Ok(result_handler(self, resp_msg)?))
        } else {
            match resp.message_type() {
                MessageType_Failure => {
                    let fail_msg = resp.into_message()?;
                    // debug!("Received failure: {:?}", fail_msg);
                    Ok(TrezorResponse::Failure(fail_msg))
                }
                MessageType_ButtonRequest => {
                    let req_msg = resp.into_message()?;
                    // trace!("Received ButtonRequest: {:?}", req_msg);
                    Ok(TrezorResponse::ButtonRequest(ButtonRequest {
                        result_handler,
                        message: req_msg,
                        client: self,
                    }))
                }
                MessageType_PinMatrixRequest => {
                    let req_msg = resp.into_message()?;
                    // trace!("Received PinMatrixRequest: {:?}", req_msg);
                    Ok(TrezorResponse::PinMatrixRequest(PinMatrixRequest {
                        result_handler,
                        message: req_msg,
                        client: self,
                    }))
                }
                mtype => {
                    // debug!(
                    // 	"Received unexpected msg type: {:?}; raw msg: {}",
                    // 	mtype,
                    // 	hex::encode(resp.into_payload())
                    // );
                    Err(Error::UnexpectedMessageType(mtype))
                }
            }
        }
    }

    /// Initialize the device.
    ///
    /// Warning: Must be called before sending requests to Trezor.
    pub async fn init_device(&mut self) -> Result<()> {
        let features = self.initialize().await?.ok()?;
        self.features = Some(features);
        Ok(())
    }

    pub async fn initialize(&mut self) -> Result<TrezorResponse<'_, Features, Features>> {
        let req = protos::Initialize::new();
        self.call(req, Box::new(|_, m| Ok(m))).await
    }

    pub async fn ping(&mut self, message: &str) -> Result<TrezorResponse<'_, (), protos::Success>> {
        let mut req = protos::Ping::new();
        req.set_message(message.to_owned());
        self.call(req, Box::new(|_, _| Ok(()))).await
    }

    /// Get address(public key hash) from Trezor.
    ///
    /// Derives keys from passed `path` (key derivation path), hashes
    /// the public key and returns it.
    pub async fn get_address(
        &mut self,
        path: &KeyDerivationPath,
    ) -> Result<TrezorResponse<'_, String, TezosAddress>> {
        let mut req = protos::TezosGetAddress::new();
        req.set_address_n(path.as_ref().to_vec());

        self.call(
            req,
            Box::new(|_, m: TezosAddress| Ok(m.get_address().to_string())),
        )
        .await
    }

    pub async fn get_komodo_address(
        &mut self,
        path: &KeyDerivationPath,
    ) -> Result<TrezorResponse<'_, String, BitcoinAddress>> {
        let mut req = protos::GetAddress::default();
        req.set_address_n(path.as_ref().to_vec());
        // req.set_coin_name("Komodo".to_owned());
        req.set_coin_name("Komodo".to_owned());

        self.call(
            req,
            Box::new(|_, m: protos::Address| Ok(m.get_address().to_string())),
        )
        .await
    }

    /// Get public key from Trezor.
    ///
    /// Derives keys from passed `path` (key derivation path) and
    /// returns public key.
    pub async fn get_public_key(
        &mut self,
        path: &KeyDerivationPath,
    ) -> Result<TrezorResponse<'_, String, TezosPublicKey>> {
        let mut req = protos::TezosGetPublicKey::new();
        req.set_address_n(path.as_ref().to_vec());

        self.call(
            req,
            Box::new(|_, m: protos::TezosPublicKey| Ok(m.get_public_key().to_string())),
        )
        .await
    }

    pub async fn sign_tx(
        &mut self,
        tx: TezosSignTx,
    ) -> Result<TrezorResponse<'_, TezosSignedTx, TezosSignedTx>> {
        self.call(tx, Box::new(|_, m| Ok(m))).await
    }
}
