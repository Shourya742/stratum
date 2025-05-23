use std::{
    convert::From,
    fmt::Debug,
    sync::{MutexGuard, PoisonError},
};

use roles_logic_sv2::parsers::Mining;

#[derive(std::fmt::Debug)]
pub enum TPError {
    Io(std::io::Error),
    ChannelSend(Box<dyn std::marker::Send + Debug>),
    ChannelRecv(async_channel::RecvError),
    BinarySv2(binary_sv2::Error),
    Codec(codec_sv2::Error),
    Noise(noise_sv2::Error),
    RolesLogic(roles_logic_sv2::Error),
    Framing(codec_sv2::framing_sv2::Error),
    PoisonLock(String),
    Custom(String),
    Sv2ProtocolError((u32, Mining<'static>)),
}

impl std::fmt::Display for TPError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use TPError::*;
        match self {
            Io(ref e) => write!(f, "I/O error: `{:?}", e),
            ChannelSend(ref e) => write!(f, "Channel send failed: `{:?}`", e),
            ChannelRecv(ref e) => write!(f, "Channel recv failed: `{:?}`", e),
            BinarySv2(ref e) => write!(f, "Binary SV2 error: `{:?}`", e),
            Codec(ref e) => write!(f, "Codec SV2 error: `{:?}", e),
            Framing(ref e) => write!(f, "Framing SV2 error: `{:?}`", e),
            Noise(ref e) => write!(f, "Noise SV2 error: `{:?}", e),
            RolesLogic(ref e) => write!(f, "Roles Logic SV2 error: `{:?}`", e),
            PoisonLock(ref e) => write!(f, "Poison lock: {:?}", e),
            Custom(ref e) => write!(f, "Custom SV2 error: `{:?}`", e),
            Sv2ProtocolError(ref e) => {
                write!(f, "Received Sv2 Protocol Error from upstream: `{:?}`", e)
            }
        }
    }
}

pub type TPResult<T> = Result<T, TPError>;

impl From<std::io::Error> for TPError {
    fn from(e: std::io::Error) -> TPError {
        TPError::Io(e)
    }
}

impl From<async_channel::RecvError> for TPError {
    fn from(e: async_channel::RecvError) -> TPError {
        TPError::ChannelRecv(e)
    }
}

impl From<binary_sv2::Error> for TPError {
    fn from(e: binary_sv2::Error) -> TPError {
        TPError::BinarySv2(e)
    }
}

impl From<codec_sv2::Error> for TPError {
    fn from(e: codec_sv2::Error) -> TPError {
        TPError::Codec(e)
    }
}

impl From<noise_sv2::Error> for TPError {
    fn from(e: noise_sv2::Error) -> TPError {
        TPError::Noise(e)
    }
}

impl From<roles_logic_sv2::Error> for TPError {
    fn from(e: roles_logic_sv2::Error) -> TPError {
        TPError::RolesLogic(e)
    }
}

impl<T: 'static + std::marker::Send + Debug> From<async_channel::SendError<T>> for TPError {
    fn from(e: async_channel::SendError<T>) -> TPError {
        TPError::ChannelSend(Box::new(e))
    }
}

impl From<String> for TPError {
    fn from(e: String) -> TPError {
        TPError::Custom(e)
    }
}
impl From<codec_sv2::framing_sv2::Error> for TPError {
    fn from(e: codec_sv2::framing_sv2::Error) -> TPError {
        TPError::Framing(e)
    }
}

impl<T> From<PoisonError<MutexGuard<'_, T>>> for TPError {
    fn from(e: PoisonError<MutexGuard<T>>) -> TPError {
        TPError::PoisonLock(e.to_string())
    }
}

impl From<(u32, Mining<'static>)> for TPError {
    fn from(e: (u32, Mining<'static>)) -> Self {
        TPError::Sv2ProtocolError(e)
    }
}
