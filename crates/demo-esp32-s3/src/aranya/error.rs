use aranya_crypto::keystore;
use aranya_policy_vm::UnsupportedVersion;
use aranya_runtime::StorageError;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
#[allow(dead_code)]
pub enum Error {
    #[error("KeyStore error: {0}")]
    KeyStore(keystore::ErrorKind),
    /* #[error("CBOR decode error: {0}")]
    Decode(#[from] minicbor_serde::error::DecodeError), */
    #[error("Import error: {0}")]
    Import(#[from] aranya_crypto::import::ImportError),
    #[error("VM policy error: {0}")]
    VM(#[from] aranya_runtime::VmPolicyError),
    #[error("Client error: {0}")]
    Client(#[from] aranya_runtime::ClientError),
    #[error("Sync error: {0}")]
    Sync(#[from] aranya_runtime::SyncError),
    #[error("Effects parse error: {0}")]
    EffectsParse(#[from] aranya_policy_ifgen::EffectsParseError),
    #[error("Network error: {0}")]
    Network(#[from] crate::net::NetworkError),
    #[error("Crypto ID error: {0}")]
    Id(#[from] aranya_crypto::id::IdError),
    #[error("Key wrapping error: {0}")]
    Wrap(#[from] aranya_crypto::WrapError),
    #[error("Public Key error: {0}")]
    Pk(#[from] aranya_crypto::signer::PkError),
    #[error("postcard error: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("rkyv error: {0}")]
    Rkyv(#[from] rkyv::rancor::Error),
    #[error("Unsupported Module Version")]
    UnsupportedVersion,
    #[error("Storage Error")]
    StorageError(#[from] aranya_runtime::StorageError),
    #[error("test")]
    Other,
}
pub type Result<T> = core::result::Result<T, Error>;

impl keystore::Error for Error {
    fn new<E>(kind: keystore::ErrorKind, _err: E) -> Self
    where
        E: core::error::Error + Send + Sync + 'static,
    {
        Self::KeyStore(kind)
    }

    fn kind(&self) -> keystore::ErrorKind {
        match self {
            Self::KeyStore(kind) => *kind,
            _ => keystore::ErrorKind::Other,
        }
    }
}

impl From<UnsupportedVersion> for Error {
    fn from(_: UnsupportedVersion) -> Self {
        Self::UnsupportedVersion
    }
}
