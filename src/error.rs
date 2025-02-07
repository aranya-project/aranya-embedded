use aranya_crypto::keystore;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("KeyStore error: {0}")]
    KeyStore(keystore::ErrorKind),
    #[error("test")]
    Other,
}

impl keystore::Error for Error {
    fn new<E>(kind: keystore::ErrorKind, err: E) -> Self
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
