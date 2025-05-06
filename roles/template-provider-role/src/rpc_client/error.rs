#[derive(Debug)]
pub enum RpcClientError {
    Connection(std::io::Error),
    Capnp(capnp::Error),
    Schema(capnp::NotInSchema),
    Encode(stratum_common::bitcoin::consensus::encode::Error),
    InvalidData(String),
    OperationFailed(String),
    NotFound,
}

impl From<std::io::Error> for RpcClientError {
    fn from(value: std::io::Error) -> Self {
        Self::Connection(value)
    }
}

impl From<capnp::Error> for RpcClientError {
    fn from(value: capnp::Error) -> Self {
        Self::Capnp(value)
    }
}

impl From<capnp::NotInSchema> for RpcClientError {
    fn from(value: capnp::NotInSchema) -> Self {
        Self::Schema(value)
    }
}

impl From<stratum_common::bitcoin::consensus::encode::Error> for RpcClientError {
    fn from(value: stratum_common::bitcoin::consensus::encode::Error) -> Self {
        Self::Encode(value)
    }
}
